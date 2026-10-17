package permissions

import (
	"fmt"

	"rozsa/backend/internal/app/settings"
)

type projectRuleStore struct {
	manager *settings.Manager
}

func NewProjectRuleStore(manager *settings.Manager) (ProjectRuleStore, error) {
	if manager == nil {
		return nil, fmt.Errorf("settings manager is required")
	}
	return &projectRuleStore{manager: manager}, nil
}

func (store *projectRuleStore) AddProjectPermissionAllow(rule string) error {
	if _, exists := store.manager.Path(settings.Project); !exists {
		return nil
	}
	overrides, err := store.manager.Overrides(settings.Project)
	if err != nil {
		return err
	}
	permission, _ := overrides["permission"].(map[string]any)
	allow := []string{}
	if values, exists := permission["allow"]; exists {
		items, ok := values.([]any)
		if !ok {
			return fmt.Errorf("project permission.allow must be an array")
		}
		for _, value := range items {
			if text, ok := value.(string); ok {
				allow = append(allow, text)
			}
		}
	}
	allow = migrateLegacyReadOnlyRules(allow)
	if containsRule(allow, rule) {
		return nil
	}
	allow = append(allow, rule)
	values := make([]any, len(allow))
	for index, value := range allow {
		values[index] = value
	}
	return store.manager.Set(settings.Project, "permission.allow", values)
}

func migrateLegacyReadOnlyRules(rules []string) []string {
	legacyFound := false
	filtered := make([]string, 0, len(rules))
	for _, rule := range rules {
		switch rule {
		case "ls(*)", "grep(*)", "find(*)":
			legacyFound = true
		default:
			filtered = append(filtered, rule)
		}
	}
	if !legacyFound {
		return filtered
	}
	for _, rule := range DefaultReadOnlyBashRules() {
		if !containsRule(filtered, rule) {
			filtered = append(filtered, rule)
		}
	}
	return filtered
}
