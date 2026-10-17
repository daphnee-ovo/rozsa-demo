package permissions

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"sync"
	"testing"

	appsettings "rozsa/backend/internal/app/settings"
)

type permissionRuleWriter struct {
	rules []string
	err   error
}

func (writer *permissionRuleWriter) AddProjectPermissionAllow(rule string) error {
	writer.rules = append(writer.rules, rule)
	return writer.err
}

func testPolicy(t *testing.T, mode Mode, root string, rules Rules) *Policy {
	t.Helper()
	policy, err := NewPolicy(mode, root, rules)
	if err != nil {
		t.Fatal(err)
	}
	return policy
}

func TestPermissionModeAndRulePrecedence(t *testing.T) {
	if mode, err := ParseMode("on-request"); err != nil || mode != OnRequest {
		t.Fatalf("ParseMode = %q, %v", mode, err)
	}
	if _, err := ParseMode("auto-permission"); err == nil {
		t.Fatal("unsupported mode accepted")
	}
	policy := testPolicy(t, OnRequest, "", Rules{
		Deny: []string{"Bash(git push *)"}, Ask: []string{"Bash(cargo test *)"}, Allow: []string{"Bash(*)"},
	})
	if got := policy.Evaluate("s", "Bash", map[string]any{"command": "git push origin main"}); got.Kind != Block {
		t.Fatalf("deny precedence: %#v", got)
	}
	if got := policy.Evaluate("s", "Bash", map[string]any{"command": "cargo test unit"}); got.Kind != NeedApproval {
		t.Fatalf("ask precedence: %#v", got)
	}
	if got := policy.Evaluate("s", "Bash", map[string]any{"command": "git status"}); got.Kind != Allow {
		t.Fatalf("allow rule: %#v", got)
	}
	compound := testPolicy(t, OnRequest, "", Rules{Allow: []string{"Bash(cargo test *)"}})
	if got := compound.Evaluate("s", "Bash", map[string]any{"command": "cargo test && git status"}); got.Kind != NeedApproval {
		t.Fatalf("partial allow covered compound command: %#v", got)
	}
	compound.Mode = Yolo
	compound.Rules.Ask = []string{"Bash(cargo test *)"}
	if got := compound.Evaluate("s", "Bash", map[string]any{"command": "cargo test unit"}); got.Kind != NeedApproval {
		t.Fatalf("yolo bypassed explicit ask: %#v", got)
	}
	policy.Mode = Yolo
	if got := policy.Evaluate("s", "Bash", map[string]any{"command": "git reset --hard"}); got.Kind != Block {
		t.Fatalf("yolo bypassed destructive block: %#v", got)
	}
}

func TestModeUpdatesKeepSessionTrustAndAutoApproveStillAsks(t *testing.T) {
	policy := testPolicy(t, AutoApprove, "", Rules{})
	args := map[string]any{"command": "cargo test"}
	if got := policy.Evaluate("s", "Bash", args); got.Kind != NeedApproval {
		t.Fatalf("unimplemented auto-approve reviewer bypassed user approval: %#v", got)
	}
	if err := policy.RecordSessionApproval("s", "Bash:cargo test"); err != nil {
		t.Fatal(err)
	}
	if err := policy.Update(OnRequest, "", Rules{Deny: []string{"Bash(git push *)"}}); err != nil {
		t.Fatal(err)
	}
	if got := policy.Evaluate("s", "Bash", args); got.Kind != Allow {
		t.Fatalf("mode update discarded session trust: %#v", got)
	}
	if _, err := NewPolicy(Mode("invalid"), "", Rules{}); err == nil {
		t.Fatal("invalid mode accepted by policy constructor")
	}
}

func TestSafeReadOnlyShellCommandsStayWithinWorkspace(t *testing.T) {
	workspace := t.TempDir()
	policy := testPolicy(t, OnRequest, workspace, Rules{Allow: DefaultReadOnlyBashRules()})
	if got := policy.Evaluate("s", "Bash", map[string]any{"command": "git status"}); got.Kind != NeedApproval {
		t.Fatalf("unknown command was auto-allowed: %#v", got)
	}
	if got := policy.Evaluate("s", "Bash", map[string]any{"command": "grep TODO src/main.go"}); got.Kind != Allow {
		t.Fatalf("workspace read was not allowed: %#v", got)
	}
	if got := policy.Evaluate("s", "Bash", map[string]any{"command": "cat ../secrets.txt"}); got.Kind != NeedApproval {
		t.Fatalf("outside path was allowed: %#v", got)
	}
	if got := policy.Evaluate("s", "Bash", map[string]any{"command": "sort -o result.txt input.txt"}); got.Kind != NeedApproval {
		t.Fatalf("writing sort option was allowed: %#v", got)
	}
	for _, command := range []string{"grep -f /etc/hosts TODO src/main.go", "diff --from-file=/etc/hosts src/main.go", "realpath --relative-to=/tmp src/main.go"} {
		if got := policy.Evaluate("s", "Bash", map[string]any{"command": command}); got.Kind != NeedApproval {
			t.Fatalf("outside option path was allowed (%s): %#v", command, got)
		}
	}
}

func TestRustReadonlyShellCommandSetMatches(t *testing.T) {
	root := t.TempDir()
	if err := os.MkdirAll(filepath.Join(root, "src"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "src", "lib.rs"), []byte("hello"), 0o600); err != nil {
		t.Fatal(err)
	}
	policy := testPolicy(t, OnRequest, root, Rules{Allow: DefaultReadOnlyBashRules()})
	commands := []string{"head -n 2 src/lib.rs", "tail -n 2 src/lib.rs", "cat src/lib.rs", "grep hello src/lib.rs", "sort src/lib.rs", "cat src/lib.rs | sort", "pwd", "ls src", "basename src/lib.rs", "dirname src/lib.rs", "realpath src/lib.rs", "readlink src/lib.rs", "stat src/lib.rs", "file src/lib.rs", "wc -l src/lib.rs", "diff src/lib.rs src/lib.rs", "cmp src/lib.rs src/lib.rs", "comm src/lib.rs src/lib.rs", "cut -f 1 src/lib.rs", "tr a-z A-Z", "uniq src/lib.rs", "strings src/lib.rs", "od src/lib.rs", "xxd src/lib.rs"}
	for _, command := range commands {
		if got := policy.Evaluate("s", "Bash", map[string]any{"command": command}); got.Kind != Allow {
			t.Errorf("expected safe command allow for %q, got %#v", command, got)
		}
	}
}

func TestPathGlobDirectRecursiveAndHomeScopes(t *testing.T) {
	root := t.TempDir()
	policy := testPolicy(t, OnRequest, root, Rules{})
	policy.Rules.Deny = []string{"Edit(docs/*.md)"}
	if got := policy.Evaluate("s", "Edit", map[string]any{"file_path": "docs/guide.md"}); got.Kind != Block {
		t.Fatalf("direct glob did not match: %#v", got)
	}
	if got := policy.Evaluate("s", "Edit", map[string]any{"file_path": "docs/nested/guide.md"}); got.Kind != NeedApproval {
		t.Fatalf("direct glob matched nested path: %#v", got)
	}
	policy.Rules.Deny = []string{"Edit(docs/**/*.md)"}
	for _, path := range []string{"docs/guide.md", "docs/nested/guide.md"} {
		if got := policy.Evaluate("s", "Edit", map[string]any{"file_path": path}); got.Kind != Block {
			t.Errorf("recursive glob did not match %q: %#v", path, got)
		}
	}
	home, err := os.UserHomeDir()
	if err != nil {
		t.Fatal(err)
	}
	policy.Rules.Deny = []string{"Edit($HOME/*.md)"}
	if got := policy.Evaluate("s", "Edit", map[string]any{"file_path": filepath.Join(home, "note.md")}); got.Kind != Block {
		t.Fatalf("home-relative glob did not match: %#v", got)
	}
	if got := policy.Evaluate("s", "Edit", map[string]any{"file_path": filepath.Join(home, "nested", "note.md")}); got.Kind != NeedApproval {
		t.Fatalf("home glob escaped direct scope: %#v", got)
	}
}

func TestReadOnlyShellDefaultsAreEditableAndKeepSafetyBoundary(t *testing.T) {
	root := t.TempDir()
	args := map[string]any{"command": "cat src/readme.md"}
	withDefaults := testPolicy(t, OnRequest, root, Rules{Allow: DefaultReadOnlyBashRules()})
	if got := withDefaults.Evaluate("s", "Bash", args); got.Kind != Allow {
		t.Fatalf("safe default command was not allowed: %#v", got)
	}
	withoutDefaults := testPolicy(t, OnRequest, root, Rules{})
	if got := withoutDefaults.Evaluate("s", "Bash", args); got.Kind != NeedApproval {
		t.Fatalf("removing default rule did not restore approval: %#v", got)
	}
	unsafe := map[string]any{"command": "cat /etc/hosts"}
	if got := withDefaults.Evaluate("s", "Bash", unsafe); got.Kind != NeedApproval {
		t.Fatalf("default readonly rule bypassed path boundary: %#v", got)
	}
}

func TestFileTrustScopesAndSymlinksStayWorkspaceBound(t *testing.T) {
	workspace := t.TempDir()
	outside := t.TempDir()
	insideDir := filepath.Join(workspace, "src", "nested")
	if err := os.MkdirAll(insideDir, 0o700); err != nil {
		t.Fatal(err)
	}
	inside := filepath.Join(workspace, "src", "lib.go")
	if err := os.WriteFile(inside, []byte("package src"), 0o600); err != nil {
		t.Fatal(err)
	}
	external := filepath.Join(outside, "secret.go")
	if err := os.WriteFile(external, []byte("secret"), 0o600); err != nil {
		t.Fatal(err)
	}
	link := filepath.Join(workspace, "linked.go")
	if err := os.Symlink(external, link); err != nil {
		t.Skipf("symlink unavailable: %v", err)
	}

	policy := testPolicy(t, OnRequest, workspace, Rules{})
	verdict := policy.Evaluate("s", "Edit", map[string]any{"file_path": inside})
	if verdict.Kind != NeedApproval {
		t.Fatalf("edit verdict = %#v", verdict)
	}
	resolvedWorkspace, err := filepath.EvalSymlinks(workspace)
	if err != nil {
		t.Fatal(err)
	}
	want := []string{filepath.ToSlash(filepath.Join(resolvedWorkspace, "src", "lib.go")), filepath.ToSlash(filepath.Join(resolvedWorkspace, "src", "*.go")), filepath.ToSlash(filepath.Join(resolvedWorkspace, "src", "*")), filepath.ToSlash(filepath.Join(resolvedWorkspace, "*"))}
	got := []string{}
	for _, level := range verdict.Info.TrustGroups[0].Levels {
		got = append(got, level.Label)
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("file trust levels = %#v, want %#v", got, want)
	}
	outsideVerdict := policy.Evaluate("s", "Read", map[string]any{"file_path": link})
	if outsideVerdict.Kind != NeedApproval || len(outsideVerdict.Info.TrustGroups[0].Levels) != 1 {
		t.Fatalf("symlink escaped workspace trust boundary: %#v", outsideVerdict)
	}

	policy.Rules.Deny = []string{"Edit(src/**/*.go)"}
	if got := policy.Evaluate("s", "Edit", map[string]any{"file_path": inside}); got.Kind != Block {
		t.Fatalf("recursive path rule did not match: %#v", got)
	}
	if got := policy.Evaluate("s", "Edit", map[string]any{"file_path": filepath.Join(workspace, "README.md")}); got.Kind != NeedApproval {
		t.Fatalf("path rule escaped its scope: %#v", got)
	}
	policy.Rules.Deny = nil
	policy.Rules.Allow = []string{"Edit(src/**/*.go)"}
	if got := policy.Evaluate("s", "Edit", map[string]any{"file_path": inside}); got.Kind != Allow {
		t.Fatalf("project path allow did not match: %#v", got)
	}
	if got := policy.Evaluate("s", "Edit", map[string]any{"file_path": link}); got.Kind != NeedApproval {
		t.Fatalf("project path allow followed an escaping symlink: %#v", got)
	}
}

func TestTrustGroupsContainOnlyUntrustedShellSegments(t *testing.T) {
	policy := testPolicy(t, OnRequest, "", Rules{})
	if err := policy.RecordSessionApproval("s", "Bash:cargo test"); err != nil {
		t.Fatal(err)
	}
	got := policy.Evaluate("s", "Bash", map[string]any{"command": "cargo test && git status"})
	if got.Kind != NeedApproval {
		t.Fatalf("verdict = %#v", got)
	}
	if len(got.Info.TrustGroups) != 1 || got.Info.TrustGroups[0].Target != "git status" {
		t.Fatalf("trust groups = %#v", got.Info.TrustGroups)
	}
	keys := []string{}
	for _, level := range got.Info.TrustLevels {
		keys = append(keys, level.Key)
	}
	if !reflect.DeepEqual(keys, []string{"Bash:git status", "Bash:git"}) {
		t.Fatalf("trust levels = %#v", keys)
	}
}

func TestCommandTrustExposesProgressivePrefixes(t *testing.T) {
	levels := commandTrustLevels("Bash", "cargo test --workspace")
	want := []string{"Bash:cargo test --workspace", "Bash:cargo test", "Bash:cargo"}
	got := []string{}
	for _, level := range levels {
		got = append(got, level.Key)
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("command trust levels = %#v, want %#v", got, want)
	}
}

func TestClosingSessionCancelsItsPendingApprovalRequests(t *testing.T) {
	owner := NewOwner()
	identities := []Identity{{SessionID: "closed", RequestID: "one"}, {SessionID: "closed", RequestID: "two"}, {SessionID: "open", RequestID: "one"}}
	for _, identity := range identities {
		if err := owner.Add(identity, ApprovalInfo{}); err != nil {
			t.Fatal(err)
		}
	}
	closed := owner.CloseSession("closed")
	if len(closed) != 2 {
		t.Fatalf("closed identities = %#v", closed)
	}
	for _, identity := range identities[:2] {
		request, exists := owner.Get(identity)
		if !exists || request.Status != Cancelled {
			t.Fatalf("closed request status = %#v, exists=%v", request, exists)
		}
		if request.Response == nil || request.Response.Kind != Deny {
			t.Fatalf("closed request did not resolve as deny: %#v", request.Response)
		}
	}
	if request, _ := owner.Get(identities[2]); request.Status != Pending {
		t.Fatalf("unrelated session changed: %#v", request)
	}
	if queue := owner.Queue("closed"); len(queue) != 0 {
		t.Fatalf("closed session retained queued requests: %#v", queue)
	}
}

func TestClosingOwnerDeniesRequestsAcrossSessions(t *testing.T) {
	owner := NewOwner()
	for _, sessionID := range []string{"a", "b"} {
		if err := owner.Add(Identity{SessionID: sessionID, RequestID: "same-id"}, ApprovalInfo{}); err != nil {
			t.Fatal(err)
		}
	}
	if closed := owner.CloseAll(); len(closed) != 2 {
		t.Fatalf("closed identities = %#v", closed)
	}
	for _, sessionID := range []string{"a", "b"} {
		request, exists := owner.Get(Identity{SessionID: sessionID, RequestID: "same-id"})
		if !exists || request.Status != Cancelled || request.Response == nil || request.Response.Kind != Deny {
			t.Fatalf("request was not denied on owner close: %#v", request)
		}
	}
}

func TestDeepCommandChecksAndRuleValidation(t *testing.T) {
	policy := testPolicy(t, OnRequest, "", Rules{})
	for _, command := range []string{"echo $(rm -rf /)", "echo $ANTHROPIC_API_KEY", "echo `sudo id`"} {
		if got := policy.Evaluate("s", "Bash", map[string]any{"command": command}); got.Kind != Block {
			t.Fatalf("deeply dangerous command was not blocked (%s): %#v", command, got)
		}
	}
	if got := policy.Evaluate("s", "Bash", map[string]any{"command": "echo $HOME"}); got.Kind == Block {
		t.Fatalf("ordinary HOME expansion was blocked: %#v", got)
	}
	if err := ValidateRule("*(*)"); err == nil {
		t.Fatal("universal tool wildcard accepted")
	}
	if err := ValidateRule("Bash(regex:[)"); err == nil {
		t.Fatal("invalid regex accepted")
	}
	if err := ValidateRuleForScope("Edit(src/*.go)", true); err == nil {
		t.Fatal("global project-relative path rule accepted")
	}
	if err := ValidateRuleForScope("Edit($HOME/../secret)", true); err == nil {
		t.Fatal("global path escape accepted")
	}
}

func TestRulesSupportRegexAndCompoundShellMatching(t *testing.T) {
	policy := testPolicy(t, OnRequest, "", Rules{Allow: []string{"Bash(regex:cargo (test|check).*)"}})
	if got := policy.Evaluate("s", "Bash", map[string]any{"command": "cargo test ./..."}); got.Kind != Allow {
		t.Fatalf("regex did not match: %#v", got)
	}
	if got := policy.Evaluate("s", "Bash", map[string]any{"command": "cargo test && rm -rf /"}); got.Kind != Block {
		t.Fatalf("dangerous segment was not blocked: %#v", got)
	}
	want := []string{`echo "one|two"`, "cat file", "grep two"}
	if got := SplitShellSegments(`echo "one|two" | cat file && grep two`); !reflect.DeepEqual(got, want) {
		t.Fatalf("segments = %#v, want %#v", got, want)
	}
}

func TestTrustPersistenceAndHintSemantics(t *testing.T) {
	if got, err := ProjectRuleForTrustKey("Bash:cargo test"); err != nil || got != "Bash(cargo test *)" {
		t.Fatalf("shell trust rule = %q, %v", got, err)
	}
	if got, err := ProjectRuleForTrustKey("Edit:/workspace/src/*.go"); err != nil || got != "Edit(/workspace/src/*.go)" {
		t.Fatalf("file trust rule = %q, %v", got, err)
	}
	if _, err := ProjectRuleForTrustKey("Bash:"); err == nil {
		t.Fatal("empty trust scope accepted")
	}
	if got := SaferAlternativeHint("Bash", map[string]any{"command": "rm file"}); !strings.Contains(got, "Do not delete") {
		t.Fatalf("delete hint = %q", got)
	}
	if got := SaferAlternativeHint("Edit", nil); !strings.Contains(got, "Read the current file") {
		t.Fatalf("edit hint = %q", got)
	}
}

func TestProjectTrustWritesAndAppliesRuleAcrossSessions(t *testing.T) {
	policy := testPolicy(t, OnRequest, "", Rules{})
	store := &permissionRuleWriter{}
	if err := policy.RecordProjectApproval("Bash:cargo test", store); err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(store.rules, []string{"Bash(cargo test *)"}) {
		t.Fatalf("persisted rules = %#v", store.rules)
	}
	if got := policy.Evaluate("later-session", "Bash", map[string]any{"command": "cargo test ./..."}); got.Kind != Allow {
		t.Fatalf("project trust not reusable across sessions: %#v", got)
	}

	store.err = fmt.Errorf("disk unavailable")
	if err := policy.RecordProjectApproval("Edit:/workspace/docs/*.md", store); err == nil || !strings.Contains(err.Error(), "disk unavailable") {
		t.Fatalf("persistence failure was hidden: %v", err)
	}
	if got := policy.Evaluate("later-session", "Edit", map[string]any{"file_path": "/workspace/docs/readme.md"}); got.Kind != Allow {
		t.Fatalf("failed persistence did not retain source Rust in-memory update semantics: %#v", got)
	}
}

func TestProjectRuleStorePersistsOnlyToProjectLayer(t *testing.T) {
	root := t.TempDir()
	globalPath := filepath.Join(root, "global.json")
	projectPath := filepath.Join(root, ".rozsa", "settings.json")
	if err := os.MkdirAll(filepath.Dir(projectPath), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(globalPath, []byte(`{"permission":{"allow":["Bash(git*)"]}}`), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(projectPath, []byte(`{"permission":{"allow":["ls(*)","Edit(src/*.go)"]}}`), 0o600); err != nil {
		t.Fatal(err)
	}
	manager, err := appsettings.Load(globalPath, &projectPath, nil)
	if err != nil {
		t.Fatal(err)
	}
	store, err := NewProjectRuleStore(manager)
	if err != nil {
		t.Fatal(err)
	}
	policy := testPolicy(t, OnRequest, "", Rules{})
	if err := policy.RecordProjectApproval("Bash:cargo test", store); err != nil {
		t.Fatal(err)
	}
	projectData, err := os.ReadFile(projectPath)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(projectData), `"Bash(cargo test *)"`) || strings.Contains(string(projectData), `"ls(*)"`) || !strings.Contains(string(projectData), `"Bash(cat*)"`) {
		t.Fatalf("project trust was not migrated and persisted: %s", projectData)
	}
	globalData, err := os.ReadFile(globalPath)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(globalData), `"Bash(git*)"`) || strings.Contains(string(globalData), "cargo test") {
		t.Fatalf("global settings changed while persisting project trust: %s", globalData)
	}
	reloaded, err := appsettings.Load(globalPath, &projectPath, nil)
	if err != nil {
		t.Fatal(err)
	}
	resolved := reloaded.Resolved()["permission"].(map[string]any)
	allowValues := resolved["allow"].([]any)
	allow := make([]string, 0, len(allowValues))
	for _, value := range allowValues {
		allow = append(allow, value.(string))
	}
	policy = testPolicy(t, OnRequest, "", Rules{Allow: allow})
	if got := policy.Evaluate("later", "Bash", map[string]any{"command": "cargo test --workspace"}); got.Kind != Allow {
		t.Fatalf("reloaded project permission did not apply: %#v", got)
	}
}

func TestSessionTrustRequiresEveryCompoundCommandSegment(t *testing.T) {
	policy := testPolicy(t, OnRequest, "", Rules{})
	if err := policy.RecordSessionApproval("one", "Bash:cargo test"); err != nil {
		t.Fatal(err)
	}
	args := map[string]any{"command": "cargo test && git status"}
	if got := policy.Evaluate("one", "Bash", args); got.Kind != NeedApproval {
		t.Fatalf("partial trust allowed whole command: %#v", got)
	}
	if err := policy.RecordSessionApproval("one", "Bash:git status"); err != nil {
		t.Fatal(err)
	}
	if got := policy.Evaluate("one", "Bash", args); got.Kind != Allow {
		t.Fatalf("complete trust not applied: %#v", got)
	}
	if got := policy.Evaluate("two", "Bash", args); got.Kind != NeedApproval {
		t.Fatalf("trust crossed session boundary: %#v", got)
	}
}

func TestPermissionResponsesOnlyPersistExplicitTrust(t *testing.T) {
	policy := testPolicy(t, OnRequest, "", Rules{})
	args := map[string]any{"command": "cargo test"}
	first := policy.Evaluate("session-a", "Bash", args)
	if first.Kind != NeedApproval {
		t.Fatalf("initial verdict = %#v", first)
	}
	if err := policy.ApplyResponse("session-a", Response{Kind: AllowOnce}); err != nil {
		t.Fatal(err)
	}
	if got := policy.Evaluate("session-a", "Bash", args); got.Kind != NeedApproval {
		t.Fatalf("AllowOnce persisted session trust: %#v", got)
	}
	if err := policy.ApplyResponse("session-a", Response{Kind: AllowSession, TrustKey: "Bash:cargo test"}); err != nil {
		t.Fatal(err)
	}
	if got := policy.Evaluate("session-a", "Bash", args); got.Kind != Allow {
		t.Fatalf("AllowSession did not persist trust: %#v", got)
	}
	if got := policy.Evaluate("session-b", "Bash", args); got.Kind != NeedApproval {
		t.Fatalf("session trust leaked: %#v", got)
	}
}

func TestPolicySupportsConcurrentEvaluationAndTrustUpdates(t *testing.T) {
	policy := testPolicy(t, OnRequest, "", Rules{})
	var workers sync.WaitGroup
	for index := 0; index < 8; index++ {
		workers.Add(2)
		go func(index int) {
			defer workers.Done()
			for iteration := 0; iteration < 50; iteration++ {
				_ = policy.RecordSessionApproval("session", fmt.Sprintf("Bash:cmd-%d", index))
			}
		}(index)
		go func() {
			defer workers.Done()
			for iteration := 0; iteration < 50; iteration++ {
				_ = policy.Evaluate("session", "Bash", map[string]any{"command": "cargo test"})
			}
		}()
	}
	workers.Wait()
}

func TestApprovalRequestsAreScopedQueuedAndRetainedUntilSuccess(t *testing.T) {
	owner := NewOwner()
	a := Identity{SessionID: "session-a", RequestID: "request-1"}
	b := Identity{SessionID: "session-a", RequestID: "request-2"}
	other := Identity{SessionID: "session-b", RequestID: "request-1"}
	if err := owner.Add(a, ApprovalInfo{ToolName: "Bash"}); err != nil {
		t.Fatal(err)
	}
	if err := owner.Add(b, ApprovalInfo{ToolName: "Edit"}); err != nil {
		t.Fatal(err)
	}
	if err := owner.Add(other, ApprovalInfo{ToolName: "Read"}); err != nil {
		t.Fatal(err)
	}
	if got, _ := owner.Get(b); got.Status != Queued {
		t.Fatalf("second same-session request status = %v", got.Status)
	}
	if err := owner.BeginResponse(b, Response{Kind: AllowOnce}); !errors.Is(err, ErrWrongStatus) {
		t.Fatalf("queued response error = %v", err)
	}
	if err := owner.BeginResponse(a, Response{Kind: DenyWithHint, Hint: "inspect first"}); err != nil {
		t.Fatal(err)
	}
	if err := owner.FinishResponse(a, false); err != nil {
		t.Fatal(err)
	}
	if got, exists := owner.Get(a); !exists || got.Status != Failed {
		t.Fatalf("failed request was lost: %#v, %v", got, exists)
	}
	if err := owner.BeginResponse(other, Response{Kind: AllowOnce}); err != nil {
		t.Fatal(err)
	}
	if err := owner.FinishResponse(other, true); err != nil {
		t.Fatal(err)
	}
	if _, exists := owner.Get(a); !exists {
		t.Fatal("response from other session removed request")
	}
	if err := owner.BeginResponse(a, Response{Kind: AllowOnce}); err != nil {
		t.Fatalf("failed request retry: %v", err)
	}
	if err := owner.FinishResponse(a, true); err != nil {
		t.Fatal(err)
	}
	if _, exists := owner.Get(a); exists {
		t.Fatal("successful retry did not remove request")
	}
	if err := owner.Mark(b, Stale); err != nil {
		t.Fatal(err)
	}
	if got, _ := owner.Get(b); got.Status != Stale {
		t.Fatalf("stale status = %v", got.Status)
	}
}
