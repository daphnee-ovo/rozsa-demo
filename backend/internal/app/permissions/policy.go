// Package permissions evaluates tool requests and tracks user approval requests.
package permissions

import (
	"fmt"
	"path/filepath"
	"regexp"
	"strings"
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

type Policy struct {
	Mode          Mode
	WorkspaceRoot string
	Rules         Rules
	SessionTrust  map[string]map[string]struct{}
}

func (p Policy) Evaluate(sessionID, tool string, args map[string]any) Verdict {
	command, _ := args["command"].(string)
	if isShell(tool) {
		for _, segment := range SplitShellSegments(command) {
			if reason := blockedCommand(segment); reason != "" {
				return Verdict{Kind: Block, Reason: reason}
			}
		}
	}
	targets := requestTargets(tool, args)
	if matchesAny(p.Rules.Deny, tool, targets) {
		return Verdict{Kind: Block, Reason: "blocked by permission.deny rule"}
	}
	if matchesAny(p.Rules.Ask, tool, targets) {
		return p.approval(tool, args, targets)
	}
	if p.Mode == Yolo || matchesAll(p.Rules.Allow, tool, targets) || p.inSessionTrust(sessionID, tool, targets) || safeRead(tool, args, p.WorkspaceRoot) || safeReadOnlyBash(tool, args, p.WorkspaceRoot) {
		return Verdict{Kind: Allow}
	}
	return p.approval(tool, args, targets)
}

func (p Policy) inSessionTrust(sessionID, tool string, targets []string) bool {
	grants := p.SessionTrust[sessionID]
	if len(targets) == 0 || len(grants) == 0 {
		return false
	}
	for _, target := range targets {
		matched := false
		for key := range grants {
			if strings.HasPrefix(key, tool+":") && commandTrustMatches(strings.TrimPrefix(key, tool+":"), target) {
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

func (p Policy) approval(tool string, args map[string]any, targets []string) Verdict {
	var summary string
	if isShell(tool) {
		summary, _ = args["command"].(string)
	} else if len(targets) != 0 {
		summary = targets[0]
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
	if isShell(tool) {
		segments := SplitShellSegments(summary)
		levels = nil
		for _, segment := range segments {
			levels = append(levels, TrustLevel{Label: segment, Key: tool + ":" + segment})
			words := strings.Fields(segment)
			for len(words) > 1 {
				words = words[:len(words)-1]
				prefix := strings.Join(words, " ")
				levels = append(levels, TrustLevel{Label: prefix, Key: tool + ":" + prefix})
			}
		}
	}
	if len(summary) > 80 {
		summary = summary[:80]
	}
	info := &ApprovalInfo{ToolName: tool, ArgsSummary: summary, Risk: risk, TrustKey: key, TrustLevels: levels, TrustGroups: []TrustGroup{{Target: summary, Levels: levels}}}
	return Verdict{Kind: NeedApproval, Info: info}
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

func requestTargets(tool string, args map[string]any) []string {
	if isShell(tool) {
		command, _ := args["command"].(string)
		return SplitShellSegments(command)
	}
	for _, key := range []string{"file_path", "path"} {
		if path, ok := args[key].(string); ok && path != "" {
			return []string{filepath.Clean(path)}
		}
	}
	return []string{fmt.Sprint(args)}
}

func matchesAny(rules []string, tool string, targets []string) bool {
	for _, rule := range rules {
		ruleTool, pattern, ok := parseRule(rule)
		if !ok || (ruleTool != "*" && !strings.EqualFold(ruleTool, tool)) {
			continue
		}
		for _, target := range targets {
			if patternMatches(pattern, target) {
				return true
			}
		}
	}
	return false
}

func matchesAll(rules []string, tool string, targets []string) bool {
	if len(targets) == 0 {
		return false
	}
	for _, target := range targets {
		if !matchesAny(rules, tool, []string{target}) {
			return false
		}
	}
	return true
}

func parseRule(rule string) (string, string, bool) {
	open := strings.IndexByte(rule, '(')
	if open < 1 || !strings.HasSuffix(rule, ")") {
		return "", "", false
	}
	tool, pattern := strings.TrimSpace(rule[:open]), strings.TrimSpace(rule[open+1:len(rule)-1])
	return tool, pattern, tool != "" && pattern != ""
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
		absolute, err := filepath.Abs(filepath.Join(root, path))
		base, baseErr := filepath.Abs(root)
		return err == nil && baseErr == nil && (absolute == base || strings.HasPrefix(absolute, base+string(filepath.Separator)))
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
		if root != "" {
			for _, field := range fields[1:] {
				if strings.HasPrefix(field, "-") || field == "-" {
					continue
				}
				if !withinRoot(field, root) {
					return false
				}
			}
		}
	}
	return true
}

func withinRoot(path, root string) bool {
	base, baseErr := filepath.Abs(root)
	candidate := path
	if !filepath.IsAbs(candidate) {
		candidate = filepath.Join(root, candidate)
	}
	abs, pathErr := filepath.Abs(candidate)
	return baseErr == nil && pathErr == nil && (abs == base || strings.HasPrefix(abs, base+string(filepath.Separator)))
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
	}
	for _, item := range patterns {
		if regexp.MustCompile(item.expression).MatchString(strings.TrimSpace(command)) {
			return item.reason
		}
	}
	return ""
}
