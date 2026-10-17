// Package sessionconfig resolves per-session model and thinking overlays.
package sessionconfig

import (
	"fmt"

	"rozsa/backend/internal/app/session"
	modeltypes "rozsa/backend/internal/model/types"
)

type Config struct {
	Model    modeltypes.Model
	Thinking modeltypes.ThinkingEffort
}
type ResolveModel func(provider, id string) (modeltypes.Model, error)

func NormalizeThinking(model modeltypes.Model, requested modeltypes.ThinkingEffort) modeltypes.ThinkingEffort {
	rank := map[modeltypes.ThinkingEffort]int{modeltypes.ThinkingOff: 0, modeltypes.ThinkingLow: 1, modeltypes.ThinkingMedium: 2, modeltypes.ThinkingHigh: 3, modeltypes.ThinkingXHigh: 4, modeltypes.ThinkingMax: 5}
	if model.SupportsThinking(requested) {
		return requested
	}
	requestedRank, known := rank[requested]
	if !known {
		return modeltypes.ThinkingOff
	}
	for candidateRank := requestedRank - 1; candidateRank >= 0; candidateRank-- {
		for effort, value := range rank {
			if value == candidateRank && model.SupportsThinking(effort) {
				return effort
			}
		}
	}
	return modeltypes.ThinkingOff
}

// Restore replays only active-branch overlay entries in append order. A bad
// persisted model reference is surfaced rather than silently using a fallback.
func Restore(manager *session.Manager, initial Config, resolve ResolveModel) (Config, error) {
	config := initial
	for _, entry := range manager.Entries() {
		switch change := entry.Payload.(type) {
		case session.ModelChange:
			model, err := resolve(change.Provider, change.ModelID)
			if err != nil {
				return Config{}, fmt.Errorf("restore model overlay %s/%s: %w", change.Provider, change.ModelID, err)
			}
			config.Model = model
			config.Thinking = NormalizeThinking(model, config.Thinking)
		case session.ThinkingEffortChange:
			effort := modeltypes.ThinkingEffort(change.Effort)
			if !validEffort(effort) {
				return Config{}, fmt.Errorf("invalid persisted thinking effort %q", change.Effort)
			}
			config.Thinking = NormalizeThinking(config.Model, effort)
		}
	}
	return config, nil
}

// SetModel persists before changing in-memory state, matching session leaf
// ordering; failed writes do not update the overlay.
func SetModel(manager *session.Manager, current Config, next modeltypes.Model) (Config, error) {
	thinking := NormalizeThinking(next, current.Thinking)
	if _, err := manager.AppendModelChange(next.Provider, next.ID); err != nil {
		return current, err
	}
	return Config{Model: next, Thinking: thinking}, nil
}
func SetThinking(manager *session.Manager, current Config, requested modeltypes.ThinkingEffort) (Config, error) {
	if !validEffort(requested) {
		return current, fmt.Errorf("invalid thinking effort %q", requested)
	}
	effort := NormalizeThinking(current.Model, requested)
	if _, err := manager.AppendThinkingEffortChange(string(effort)); err != nil {
		return current, err
	}
	current.Thinking = effort
	return current, nil
}
func validEffort(effort modeltypes.ThinkingEffort) bool {
	switch effort {
	case modeltypes.ThinkingOff, modeltypes.ThinkingLow, modeltypes.ThinkingMedium, modeltypes.ThinkingHigh, modeltypes.ThinkingXHigh, modeltypes.ThinkingMax:
		return true
	}
	return false
}
