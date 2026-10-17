package permissions_test

import (
	"testing"

	"rozsa/backend/internal/app/permissions"
)

type projectRuleRecorder struct{ rule string }

func (recorder *projectRuleRecorder) AddProjectPermissionAllow(rule string) error {
	recorder.rule = rule
	return nil
}

func TestPublicFactoryReturnsAbstractPolicyContract(t *testing.T) {
	var policy permissions.Policy
	created, err := permissions.NewPolicy(permissions.OnRequest, t.TempDir(), permissions.Rules{})
	if err != nil {
		t.Fatal(err)
	}
	policy = created

	if verdict := policy.Evaluate("session", "Bash", map[string]any{"command": "cargo test"}); verdict.Kind != permissions.NeedApproval {
		t.Fatalf("initial evaluation = %#v", verdict)
	}
	if err := policy.ApplyResponse("session", permissions.Response{Kind: permissions.AllowSession, TrustKey: "Bash:cargo test"}); err != nil {
		t.Fatal(err)
	}
	if verdict := policy.Evaluate("session", "Bash", map[string]any{"command": "cargo test"}); verdict.Kind != permissions.Allow {
		t.Fatalf("session response not applied: %#v", verdict)
	}
	if err := policy.Update(permissions.OnRequest, t.TempDir(), permissions.Rules{Deny: []string{"Bash(git push *)"}}); err != nil {
		t.Fatal(err)
	}
	if verdict := policy.Evaluate("another-session", "Bash", map[string]any{"command": "git push origin main"}); verdict.Kind != permissions.Block {
		t.Fatalf("updated policy = %#v", verdict)
	}
	store := &projectRuleRecorder{}
	if err := policy.RecordProjectApproval("Bash:cargo check", store); err != nil {
		t.Fatal(err)
	}
	if store.rule != "Bash(cargo check *)" {
		t.Fatalf("persisted project rule = %q", store.rule)
	}
}
