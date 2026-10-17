// Package permissions evaluates tool requests and tracks user approval requests.
package permissions

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"sync"
)

type Mode string

const (
	OnRequest   Mode = "on-request"
	AutoApprove Mode = "auto-approve"
	Yolo        Mode = "yolo"
)

func ParseMode(value string) (Mode, error) {
	mode := Mode(value)
	switch mode {
	case OnRequest, AutoApprove, Yolo:
		return mode, nil
	default:
		return "", fmt.Errorf("unsupported permission mode: %s", value)
	}
}

type VerdictKind uint8

const (
	Allow VerdictKind = iota
	Block
	NeedApproval
)

type Verdict struct {
	Kind   VerdictKind
	Reason string
	Info   *ApprovalInfo
}

type Risk string

const (
	RiskRead        Risk = "read"
	RiskWrite       Risk = "write"
	RiskShell       Risk = "shell"
	RiskNetwork     Risk = "network"
	RiskGit         Risk = "git"
	RiskUnknown     Risk = "unknown"
	RiskDestructive Risk = "destructive"
)

type TrustLevel struct {
	Label string
	Key   string
}

type TrustGroup struct {
	Target string
	Levels []TrustLevel
}

type ApprovalInfo struct {
	ToolName    string
	ArgsSummary string
	Risk        Risk
	TrustKey    string
	TrustLevels []TrustLevel
	TrustGroups []TrustGroup
}

type ResponseKind uint8

const (
	AllowOnce ResponseKind = iota
	AllowSession
	Deny
	DenyWithHint
)

type Response struct {
	Kind     ResponseKind
	TrustKey string
	Hint     string
}

type Rules struct {
	Deny  []string
	Ask   []string
	Allow []string
}

type Policy interface {
	Evaluate(sessionID, tool string, args map[string]any) Verdict
	Update(mode Mode, workspaceRoot string, rules Rules) error
	ApplyResponse(sessionID string, response Response) error
	RecordProjectApproval(trustKey string, store ProjectRuleStore) error
}

type policy struct {
	mode          Mode
	workspaceRoot string
	rules         Rules
	mu            sync.RWMutex
	sessionTrust  map[string]map[string]struct{}
}

// ProjectRuleStore persists an approved trust scope in the project settings.
type ProjectRuleStore interface {
	AddProjectPermissionAllow(rule string) error
}

func NewPolicy(mode Mode, workspaceRoot string, rules Rules) (Policy, error) {
	if _, err := ParseMode(string(mode)); err != nil {
		return nil, err
	}
	if workspaceRoot == "" {
		current, err := os.Getwd()
		if err != nil {
			return nil, fmt.Errorf("resolve permission workspace: %w", err)
		}
		workspaceRoot = current
	}
	return &policy{mode: mode, workspaceRoot: workspaceRoot, rules: cloneRules(rules), sessionTrust: make(map[string]map[string]struct{})}, nil
}

func (p *policy) Update(mode Mode, workspaceRoot string, rules Rules) error {
	if _, err := ParseMode(string(mode)); err != nil {
		return err
	}
	if workspaceRoot == "" {
		current, err := os.Getwd()
		if err != nil {
			return fmt.Errorf("resolve permission workspace: %w", err)
		}
		workspaceRoot = current
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	p.mode, p.workspaceRoot, p.rules = mode, workspaceRoot, cloneRules(rules)
	return nil
}

func cloneRules(rules Rules) Rules {
	return Rules{Deny: append([]string(nil), rules.Deny...), Ask: append([]string(nil), rules.Ask...), Allow: append([]string(nil), rules.Allow...)}
}

func (p *policy) recordSessionApproval(sessionID, trustKey string) error {
	if sessionID == "" || trustKey == "" {
		return fmt.Errorf("session approval requires session and trust identity")
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.sessionTrust == nil {
		p.sessionTrust = make(map[string]map[string]struct{})
	}
	if p.sessionTrust[sessionID] == nil {
		p.sessionTrust[sessionID] = make(map[string]struct{})
	}
	p.sessionTrust[sessionID][trustKey] = struct{}{}
	return nil
}

func (p *policy) ApplyResponse(sessionID string, response Response) error {
	switch response.Kind {
	case AllowOnce, Deny, DenyWithHint:
		return nil
	case AllowSession:
		return p.recordSessionApproval(sessionID, response.TrustKey)
	default:
		return fmt.Errorf("unsupported permission response kind: %d", response.Kind)
	}
}

func (p *policy) RecordProjectApproval(trustKey string, store ProjectRuleStore) error {
	if store == nil {
		return fmt.Errorf("project permission store is required")
	}
	rule, err := ProjectRuleForTrustKey(trustKey)
	if err != nil {
		return err
	}
	p.mu.Lock()
	if !containsRule(p.rules.Allow, rule) {
		p.rules.Allow = append(p.rules.Allow, rule)
	}
	p.mu.Unlock()
	if err := store.AddProjectPermissionAllow(rule); err != nil {
		return fmt.Errorf("persist project permission rule: %w", err)
	}
	return nil
}

func containsRule(rules []string, wanted string) bool {
	for _, rule := range rules {
		if rule == wanted {
			return true
		}
	}
	return false
}

func (p *policy) Evaluate(sessionID, tool string, args map[string]any) Verdict {
	p.mu.RLock()
	defer p.mu.RUnlock()
	command, _ := args["command"].(string)
	if isShell(tool) {
		if reason := blockedCommandDeep(command); reason != "" {
			return Verdict{Kind: Block, Reason: reason}
		}
		for _, segment := range SplitShellSegments(command) {
			if reason := blockedCommand(segment); reason != "" {
				return Verdict{Kind: Block, Reason: reason}
			}
		}
	}
	targets := requestTargets(tool, args, p.workspaceRoot)
	if matchesAny(p.rules.Deny, tool, targets, p.workspaceRoot) {
		return Verdict{Kind: Block, Reason: "blocked by permission.deny rule"}
	}
	if matchesAny(p.rules.Ask, tool, targets, p.workspaceRoot) {
		return p.approval(sessionID, tool, args, targets)
	}
	if p.mode == Yolo || matchesAllAllow(p.rules.Allow, tool, targets, p.workspaceRoot, args) || p.inSessionTrust(sessionID, tool, targets) || safeRead(tool, args, p.workspaceRoot) {
		return Verdict{Kind: Allow}
	}
	return p.approval(sessionID, tool, args, targets)
}

func DefaultAllowRules() []string {
	rules := []string{"subagent(*)", "askUserQuestion(*)"}
	return append(rules, DefaultReadOnlyBashRules()...)
}

func DefaultReadOnlyBashRules() []string {
	rules := []string{}
	for _, command := range []string{"head", "tail", "cat", "grep", "sort", "pwd", "ls", "basename", "dirname", "realpath", "readlink", "stat", "file", "wc", "diff", "cmp", "comm", "cut", "tr", "uniq", "strings", "od", "xxd"} {
		rules = append(rules, "Bash("+command+"*)")
	}
	return rules
}

func (p *policy) inSessionTrust(sessionID, tool string, targets []string) bool {
	grants := p.sessionTrust[sessionID]
	if len(targets) == 0 || len(grants) == 0 {
		return false
	}
	for _, target := range targets {
		matched := false
		for key := range grants {
			grant, ok := strings.CutPrefix(key, tool+":")
			if ok && trustScopeMatches(tool, grant, target) {
				matched = true
				break
			}
		}
		if !matched {
			return false
		}
	}
	return true
}

func (p *policy) approval(sessionID, tool string, args map[string]any, targets []string) Verdict {
	var summary string
	if isShell(tool) {
		summary, _ = args["command"].(string)
		if summary == "" {
			summary = "(no command)"
		}
	} else if len(targets) != 0 {
		summary = targets[0]
	} else if isPathTool(tool) {
		summary = "(unknown)"
	} else {
		summary = fmt.Sprint(args)
	}
	key := tool + ":" + summary
	risk := RiskUnknown
	switch strings.ToLower(tool) {
	case "read":
		risk = RiskRead
	case "write", "edit":
		risk = RiskWrite
	case "bash":
		risk = RiskShell
	}
	if isShell(tool) {
		trimmed := strings.TrimSpace(summary)
		if strings.HasPrefix(trimmed, "git ") {
			risk = RiskGit
		}
		for _, prefix := range []string{"curl ", "wget ", "npm install", "npm publish", "pnpm ", "yarn ", "bun install", "pip install", "cargo install"} {
			if strings.Contains(trimmed, prefix) {
				risk = RiskNetwork
				break
			}
		}
	}
	if strings.EqualFold(tool, "read") || strings.EqualFold(tool, "write") || strings.EqualFold(tool, "edit") {
		path, _ := args["file_path"].(string)
		for _, marker := range []string{".env", "id_rsa", "id_ed25519", ".npmrc", ".pypirc", "credentials", "secrets", ".aws/credentials", "token", ".netrc"} {
			if strings.Contains(strings.ToLower(path), marker) {
				risk = RiskDestructive
				break
			}
		}
	}
	levels := []TrustLevel{{Label: summary, Key: key}}
	groups := []TrustGroup{}
	if isShell(tool) {
		segments := SplitShellSegments(summary)
		levels = nil
		for _, segment := range segments {
			if p.inSessionTrust(sessionID, tool, []string{segment}) || matchesAnyAllow(p.rules.Allow, tool, segment, p.workspaceRoot, args) {
				continue
			}
			segmentLevels := commandTrustLevels(tool, segment)
			groups = append(groups, TrustGroup{Target: segment, Levels: segmentLevels})
			levels = append(levels, segmentLevels...)
		}
	} else if isPathTool(tool) {
		path, _ := args["file_path"].(string)
		if path == "" {
			path, _ = args["path"].(string)
		}
		canonical := canonicalPath(path, p.workspaceRoot)
		if !p.inSessionTrust(sessionID, tool, []string{canonical}) && !matchesAnyAllow(p.rules.Allow, tool, canonical, p.workspaceRoot, args) {
			levels = fileTrustLevels(tool, path, p.workspaceRoot)
			groups = []TrustGroup{{Target: canonical, Levels: levels}}
		} else {
			levels = nil
		}
	}
	if len(groups) == 0 {
		groups = []TrustGroup{{Target: summary, Levels: levels}}
	}
	summary = truncateRunes(summary, 80)
	if len(levels) > 0 {
		key = levels[0].Key
	}
	info := &ApprovalInfo{ToolName: tool, ArgsSummary: summary, Risk: risk, TrustKey: key, TrustLevels: levels, TrustGroups: groups}
	return Verdict{Kind: NeedApproval, Info: info}
}

func truncateRunes(value string, limit int) string {
	runes := []rune(value)
	if len(runes) > limit {
		return string(runes[:limit])
	}
	return value
}

func commandTrustLevels(tool, command string) []TrustLevel {
	command = strings.TrimSpace(command)
	if command == "" {
		return nil
	}
	levels := []TrustLevel{{Label: command, Key: tool + ":" + command}}
	words := strings.Fields(command)
	for count := len(words) - 1; count > 0; count-- {
		prefix := strings.Join(words[:count], " ")
		levels = append(levels, TrustLevel{Label: prefix + " *", Key: tool + ":" + prefix})
	}
	return levels
}

func fileTrustLevels(tool, path, root string) []TrustLevel {
	absolute := canonicalPath(path, root)
	workspace := canonicalPath(root, root)
	if absolute != workspace && !withinRoot(absolute, workspace) {
		return []TrustLevel{{Label: absolute, Key: tool + ":" + absolute}}
	}
	levels := []TrustLevel{{Label: absolute, Key: tool + ":" + absolute}}
	directory := filepath.Dir(absolute)
	if extension := filepath.Ext(absolute); extension != "" {
		scope := filepath.ToSlash(filepath.Join(directory, "*"+extension))
		levels = append(levels, TrustLevel{Label: scope, Key: tool + ":" + scope})
	}
	levels = append(levels, TrustLevel{Label: filepath.ToSlash(filepath.Join(directory, "*")), Key: tool + ":" + filepath.ToSlash(filepath.Join(directory, "*"))})
	projectScope := filepath.ToSlash(filepath.Join(workspace, "*"))
	if levels[len(levels)-1].Label != projectScope {
		levels = append(levels, TrustLevel{Label: projectScope, Key: tool + ":" + projectScope})
	}
	return levels
}

func isPathTool(tool string) bool {
	switch strings.ToLower(tool) {
	case "read", "write", "edit":
		return true
	}
	return false
}

func SaferAlternativeHint(tool string, args map[string]any) string {
	if isShell(tool) {
		command, _ := args["command"].(string)
		for _, word := range strings.Fields(command) {
			if word == "rm" {
				return "Do not delete files. Inspect them first or move them to a temporary directory."
			}
		}
		if strings.Contains(command, "git push") || strings.Contains(command, "git reset") {
			return "Use a safer git command that does not publish or discard changes."
		}
		return "Use a safer command, or split the operation and start with a read-only check."
	}
	if strings.EqualFold(tool, "write") || strings.EqualFold(tool, "edit") {
		return "Read the current file first, then propose a smaller and clearly scoped change."
	}
	return "Choose a safer alternative that needs fewer permissions."
}

func ProjectRuleForTrustKey(trustKey string) (string, error) {
	tool, target, ok := strings.Cut(trustKey, ":")
	if !ok {
		return "", fmt.Errorf("Invalid trust key: %s", trustKey)
	}
	if target == "" {
		return "", fmt.Errorf("Cannot persist an empty trust scope")
	}
	if isShell(tool) {
		target += " *"
	}
	return tool + "(" + target + ")", nil
}

func requestTargets(tool string, args map[string]any, root string) []string {
	if isShell(tool) {
		command, _ := args["command"].(string)
		return SplitShellSegments(command)
	}
	for _, key := range []string{"file_path", "path"} {
		if path, ok := args[key].(string); ok && path != "" {
			return []string{canonicalPath(path, root)}
		}
	}
	return []string{fmt.Sprint(args)}
}

func matchesAny(rules []string, tool string, targets []string, root string) bool {
	for _, rule := range rules {
		ruleTool, pattern, ok := parseRule(rule)
		if !ok || (ruleTool != "*" && !strings.EqualFold(ruleTool, tool)) {
			continue
		}
		for _, target := range targets {
			matched := patternMatches(pattern, target)
			if isPathTool(tool) {
				matched = pathPatternMatches(pattern, target, root)
			}
			if matched {
				return true
			}
		}
	}
	return false
}

func matchesAll(rules []string, tool string, targets []string, root string) bool {
	if len(targets) == 0 {
		return false
	}
	for _, target := range targets {
		if !matchesAny(rules, tool, []string{target}, root) {
			return false
		}
	}
	return true
}

func matchesAllAllow(rules []string, tool string, targets []string, root string, args map[string]any) bool {
	if len(targets) == 0 {
		return false
	}
	for _, target := range targets {
		if !matchesAnyAllow(rules, tool, target, root, args) {
			return false
		}
	}
	return true
}

func matchesAnyAllow(rules []string, tool, target, root string, args map[string]any) bool {
	for _, rule := range rules {
		ruleTool, pattern, ok := parseRule(rule)
		if !ok || (ruleTool != "*" && !strings.EqualFold(ruleTool, tool)) {
			continue
		}
		if isShell(tool) && isDefaultReadonlyRule(rule) && !safeReadOnlyBash(tool, args, root) {
			continue
		}
		if patternMatches(pattern, target) || (isPathTool(tool) && pathPatternMatches(pattern, target, root)) {
			return true
		}
	}
	return false
}

func isDefaultReadonlyRule(rule string) bool {
	tool, pattern, ok := parseRule(rule)
	if !ok || !strings.EqualFold(tool, "Bash") {
		return false
	}
	for _, command := range []string{"head", "tail", "cat", "grep", "sort", "pwd", "ls", "basename", "dirname", "realpath", "readlink", "stat", "file", "wc", "diff", "cmp", "comm", "cut", "tr", "uniq", "strings", "od", "xxd"} {
		if pattern == command+"*" {
			return true
		}
	}
	return false
}

func parseRule(rule string) (string, string, bool) {
	open := strings.IndexByte(rule, '(')
	if open < 1 || !strings.HasSuffix(rule, ")") {
		return "", "", false
	}
	tool, pattern := strings.TrimSpace(rule[:open]), strings.TrimSpace(rule[open+1:len(rule)-1])
	return tool, pattern, tool != "" && pattern != ""
}

func ValidateRule(rule string) error {
	tool, pattern, ok := parseRule(rule)
	if !ok {
		return fmt.Errorf("permission rule must use Tool(pattern) syntax")
	}
	if tool == "*" && pattern == "*" {
		return fmt.Errorf("*(*) is not allowed; use yolo mode to allow every tool")
	}
	regexPattern := strings.TrimPrefix(pattern, "$HOME/")
	if expression, exists := strings.CutPrefix(regexPattern, "regex:"); exists {
		if expression == "" {
			return fmt.Errorf("regular expression cannot be empty")
		}
		if _, err := regexp.Compile("^(?:" + expression + ")$"); err != nil {
			return fmt.Errorf("invalid permission rule regular expression: %w", err)
		}
	}
	return nil
}

func ValidateRuleForScope(rule string, global bool) error {
	if err := ValidateRule(rule); err != nil {
		return err
	}
	tool, pattern, _ := parseRule(rule)
	if global && (isPathTool(tool) || tool == "*") && !strings.HasPrefix(pattern, "$HOME/") {
		return fmt.Errorf("global path rules must start with $HOME/")
	}
	if homeRelative, ok := strings.CutPrefix(pattern, "$HOME/"); ok {
		if !global {
			return fmt.Errorf("$HOME/ path rules are only valid in global settings")
		}
		for _, component := range strings.Split(filepath.ToSlash(homeRelative), "/") {
			if component == ".." {
				return fmt.Errorf("global path rules cannot escape $HOME/ with '..'")
			}
		}
	}
	return nil
}

func patternMatches(pattern, target string) bool {
	if pattern == "*" {
		return true
	}
	if expression, ok := strings.CutPrefix(pattern, "regex:"); ok {
		compiled, err := regexp.Compile("^(?:" + expression + ")$")
		return err == nil && compiled.MatchString(strings.TrimSpace(target))
	}
	matched, err := filepath.Match(pattern, strings.TrimSpace(target))
	if err == nil && matched {
		return true
	}
	// Permission globs use '*' across shell arguments, unlike filepath.Match.
	quoted := regexp.QuoteMeta(pattern)
	quoted = strings.ReplaceAll(quoted, `\*`, `.*`)
	quoted = strings.ReplaceAll(quoted, `\?`, `.`)
	compiled, err := regexp.Compile("^(?:" + quoted + ")$")
	return err == nil && compiled.MatchString(strings.TrimSpace(target))
}

func commandTrustMatches(grant, command string) bool {
	grant, command = strings.TrimSpace(grant), strings.TrimSpace(command)
	return grant == command || (strings.HasPrefix(command, grant) && len(command) > len(grant) && command[len(grant)] == ' ')
}

func isShell(tool string) bool { return strings.EqualFold(tool, "bash") }

func safeRead(tool string, args map[string]any, root string) bool {
	if strings.EqualFold(tool, "read") {
		path, _ := args["file_path"].(string)
		if path == "" {
			path, _ = args["path"].(string)
		}
		if root == "" || path == "" {
			return false
		}
		return withinRoot(path, root)
	}
	return false
}

func safeReadOnlyBash(tool string, args map[string]any, root string) bool {
	if !isShell(tool) {
		return false
	}
	command, _ := args["command"].(string)
	if command == "" || strings.ContainsAny(command, "\n\r;&<>$`()\\*?[]") {
		return false
	}
	allowed := map[string]struct{}{
		"head": {}, "tail": {}, "cat": {}, "grep": {}, "sort": {}, "pwd": {}, "ls": {}, "basename": {}, "dirname": {}, "realpath": {},
		"readlink": {}, "stat": {}, "file": {}, "wc": {}, "diff": {}, "cmp": {}, "comm": {}, "cut": {}, "tr": {}, "uniq": {}, "strings": {}, "od": {}, "xxd": {},
	}
	segments := SplitShellSegments(command)
	if len(segments) == 0 {
		return false
	}
	for _, segment := range segments {
		fields := strings.Fields(segment)
		if len(fields) == 0 {
			return false
		}
		if _, ok := allowed[fields[0]]; !ok {
			return false
		}
		if fields[0] == "tail" {
			for _, field := range fields[1:] {
				if field == "-f" || field == "-F" || field == "--follow" {
					return false
				}
			}
		}
		if fields[0] == "sort" {
			for _, field := range fields[1:] {
				if strings.HasPrefix(field, "-o") || strings.HasPrefix(field, "--output") || strings.HasPrefix(field, "-T") || strings.HasPrefix(field, "--temporary-directory") {
					return false
				}
			}
		}
		if !readonlyArgumentsWithinRoot(fields[0], fields[1:], root) {
			return false
		}
	}
	return true
}

func withinRoot(path, root string) bool {
	base := canonicalPath(root, root)
	candidate := canonicalPath(path, root)
	return candidate == base || strings.HasPrefix(candidate, strings.TrimSuffix(base, "/")+"/")
}

func readonlyArgumentsWithinRoot(executable string, arguments []string, root string) bool {
	if root == "" {
		return true
	}
	positional := []string{}
	optionValue := false
	for _, token := range arguments {
		if optionValue {
			if token != "-" && !withinRoot(token, root) {
				return false
			}
			optionValue = false
			continue
		}
		if token == "--" {
			continue
		}
		if strings.HasPrefix(token, "-") {
			if executable == "grep" && (token == "-f" || token == "--file") {
				optionValue = true
				continue
			}
			for _, option := range []string{"--from-file=", "--to-file=", "--relative-to=", "--relative-base=", "--file-list=", "--files-from="} {
				if strings.HasPrefix(token, option) {
					path := strings.TrimPrefix(token, option)
					if path != "-" && !withinRoot(path, root) {
						return false
					}
				}
			}
			if executable == "grep" && strings.HasPrefix(token, "--file=") {
				path := strings.TrimPrefix(token, "--file=")
				if path != "-" && !withinRoot(path, root) {
					return false
				}
			}
			continue
		}
		positional = append(positional, token)
	}
	if optionValue {
		return false
	}
	if executable == "grep" && len(positional) > 0 {
		positional = positional[1:]
	}
	for _, path := range positional {
		if path != "-" && !withinRoot(path, root) {
			return false
		}
	}
	return true
}

func blockedCommand(command string) string {
	patterns := []struct{ expression, reason string }{
		{`\brm\s+-[^\n;]*r[^\n;]*f[^\n;]*(/|~|\$HOME|\.|\*)(\b|$)`, "destructive: rm -rf on dangerous path"},
		{`\brm\s+[^\n;]*\*`, "destructive: rm with wildcards"},
		{`^\s*sudo\b`, "sudo requires manual execution"},
		{`\bgit\s+reset\s+--hard\b`, "destructive: git reset --hard"},
		{`\bgit\s+clean\s+-fd\b`, "destructive: git clean -fd"},
		{`\bgit\s+push\b[^\n;]*(--force|-f)\b`, "destructive: force push"},
		{`\bdd\b`, "destructive: dd disk utility"},
		{`\bmkfs\b`, "destructive: filesystem format"},
		{`\bdiskutil\s+erase\b`, "destructive: diskutil erase"},
	}
	for _, item := range patterns {
		if regexp.MustCompile(item.expression).MatchString(strings.TrimSpace(command)) {
			return item.reason
		}
	}
	return ""
}

func blockedCommandDeep(command string) string {
	if reason := blockedCommand(command); reason != "" {
		return reason
	}
	for _, nested := range commandSubstitutions(command) {
		if reason := blockedCommand(nested); reason != "" {
			return reason
		}
	}
	variables := []string{"AWS_SECRET_ACCESS_KEY", "AWS_SESSION_TOKEN", "ANTHROPIC_API_KEY", "OPENAI_API_KEY", "GITHUB_TOKEN", "GH_TOKEN", "GOOGLE_API_KEY", "API_KEY", "API_SECRET", "SECRET_KEY", "PRIVATE_KEY", "DATABASE_URL", "DB_PASSWORD", "NPM_TOKEN", "PYPI_TOKEN"}
	leakContexts := []string{"echo", "printf", "curl", "wget", "cat", "tee"}
	for _, name := range variables {
		if !strings.Contains(command, "$"+name) && !strings.Contains(command, "${"+name+"}") {
			continue
		}
		for _, context := range leakContexts {
			if strings.Contains(command, context) {
				return "blocked: potential sensitive environment variable leak"
			}
		}
	}
	return ""
}

func commandSubstitutions(command string) []string {
	result := []string{}
	for index := 0; index+1 < len(command); {
		if command[index] == '$' && command[index+1] == '(' {
			start, depth := index+2, 1
			index += 2
			for index < len(command) && depth > 0 {
				if command[index] == '(' {
					depth++
				}
				if command[index] == ')' {
					depth--
				}
				if depth > 0 {
					index++
				}
			}
			if start < index {
				result = append(result, command[start:index])
			}
			if index < len(command) {
				index++
			}
			continue
		}
		if command[index] == '`' {
			start := index + 1
			index = start
			for index < len(command) && command[index] != '`' {
				index++
			}
			if start < index {
				result = append(result, command[start:index])
			}
			if index < len(command) {
				index++
			}
			continue
		}
		index++
	}
	return result
}
