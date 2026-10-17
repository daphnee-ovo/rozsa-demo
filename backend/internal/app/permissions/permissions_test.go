package permissions

import (
	"errors"
	"reflect"
	"strings"
	"testing"
)

func TestPermissionModeAndRulePrecedence(t *testing.T) {
	if mode, err := ParseMode("on-request"); err != nil || mode != OnRequest {
		t.Fatalf("ParseMode = %q, %v", mode, err)
	}
	if _, err := ParseMode("auto-permission"); err == nil {
		t.Fatal("unsupported mode accepted")
	}
	policy := Policy{Mode: OnRequest, Rules: Rules{
		Deny: []string{"Bash(git push *)"}, Ask: []string{"Bash(cargo test *)"}, Allow: []string{"Bash(*)"},
	}}
	if got := policy.Evaluate("s", "Bash", map[string]any{"command": "git push origin main"}); got.Kind != Block {
		t.Fatalf("deny precedence: %#v", got)
	}
	if got := policy.Evaluate("s", "Bash", map[string]any{"command": "cargo test unit"}); got.Kind != NeedApproval {
		t.Fatalf("ask precedence: %#v", got)
	}
	if got := policy.Evaluate("s", "Bash", map[string]any{"command": "git status"}); got.Kind != Allow {
		t.Fatalf("allow rule: %#v", got)
	}
	compound := Policy{Mode: OnRequest, Rules: Rules{Allow: []string{"Bash(cargo test *)"}}}
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

func TestSafeReadOnlyShellCommandsStayWithinWorkspace(t *testing.T) {
	policy := Policy{Mode: OnRequest, WorkspaceRoot: "/workspace"}
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
}

func TestRulesSupportRegexAndCompoundShellMatching(t *testing.T) {
	policy := Policy{Mode: OnRequest, Rules: Rules{Allow: []string{"Bash(regex:cargo (test|check).*)"}}}
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

func TestSessionTrustRequiresEveryCompoundCommandSegment(t *testing.T) {
	policy := Policy{Mode: OnRequest, SessionTrust: map[string]map[string]struct{}{"one": {"Bash:cargo test": {}}}}
	args := map[string]any{"command": "cargo test && git status"}
	if got := policy.Evaluate("one", "Bash", args); got.Kind != NeedApproval {
		t.Fatalf("partial trust allowed whole command: %#v", got)
	}
	policy.SessionTrust["one"]["Bash:git status"] = struct{}{}
	if got := policy.Evaluate("one", "Bash", args); got.Kind != Allow {
		t.Fatalf("complete trust not applied: %#v", got)
	}
	if got := policy.Evaluate("two", "Bash", args); got.Kind != NeedApproval {
		t.Fatalf("trust crossed session boundary: %#v", got)
	}
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
