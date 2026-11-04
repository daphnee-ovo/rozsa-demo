package types_test

import (
	"encoding/json"
	"testing"

	. "rozsa/backend/internal/model/types"
)

func TestThinkingSupportDistinguishesMissingAndExplicitNull(t *testing.T) {
	var model Model
	if err := json.Unmarshal([]byte(`{"reasoning":true}`), &model); err != nil {
		t.Fatal(err)
	}
	if !model.SupportsThinking(ThinkingHigh) {
		t.Fatal("unconfigured reasoning model should retain default support")
	}
	if err := json.Unmarshal([]byte(`{"reasoning":true,"thinkingEffortMap":{"low":"light","high":null}}`), &model); err != nil {
		t.Fatal(err)
	}
	if !model.SupportsThinking(ThinkingLow) || model.SupportsThinking(ThinkingHigh) {
		t.Fatalf("explicit thinking map not honored: %#v", model.ThinkingEffortMap)
	}
}
