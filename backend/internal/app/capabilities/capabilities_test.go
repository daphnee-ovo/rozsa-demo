package capabilities

import "testing"

func TestLayeredOverridesAndResetByOmission(t *testing.T) {
	catalog := New(map[string]bool{"bash": false, "read": true}, map[string]bool{"bash": true}, map[string]bool{"read": false})
	if !catalog.Enabled("bash") {
		t.Fatal("project override should enable bash")
	}
	if catalog.Enabled("read") {
		t.Fatal("local override should disable read")
	}
	if !catalog.Enabled("unconfigured") {
		t.Fatal("missing capability should default to enabled")
	}
	if got := catalog.Overrides(Global); len(got) != 2 || got["bash"] {
		t.Fatalf("global overrides = %#v", got)
	}
	if got := catalog.Overrides(Project); len(got) != 1 || !got["bash"] {
		t.Fatalf("project overrides = %#v", got)
	}
	if got := catalog.Overrides(Local); len(got) != 1 || got["read"] {
		t.Fatalf("local overrides = %#v", got)
	}
}

func TestCatalogCopiesInputsAndReturnedOverrides(t *testing.T) {
	global := map[string]bool{"bash": false}
	catalog := New(global, nil, nil)
	global["bash"] = true
	if catalog.Enabled("bash") {
		t.Fatal("catalog retained mutable input map")
	}
	returned := catalog.Overrides(Global)
	returned["bash"] = true
	if catalog.Enabled("bash") {
		t.Fatal("returned overrides mutated catalog")
	}
}

func TestValidateNameMatchesSettingsContract(t *testing.T) {
	if err := ValidateName("  "); err == nil {
		t.Fatal("blank capability name should fail")
	}
	if err := ValidateName("read"); err != nil {
		t.Fatalf("valid name failed: %v", err)
	}
}

func TestWithOverrideAndResetRevealInheritedValue(t *testing.T) {
	catalog := New(map[string]bool{"bash": false}, nil, nil)
	enabled := true
	updated, err := catalog.WithOverride(Project, "bash", &enabled)
	if err != nil || !updated.Enabled("bash") {
		t.Fatalf("project override failed: enabled=%v err=%v", updated.Enabled("bash"), err)
	}
	reset, err := updated.WithOverride(Project, "bash", nil)
	if err != nil || reset.Enabled("bash") {
		t.Fatalf("reset did not reveal global setting: enabled=%v err=%v", reset.Enabled("bash"), err)
	}
	if catalog.Enabled("bash") {
		t.Fatal("immutable update modified original catalog")
	}
}
