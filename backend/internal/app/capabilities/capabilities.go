// Package capabilities resolves inherited tool and skill enablement.
package capabilities

import (
	"errors"
	"strings"
)

type Scope uint8

const (
	Global Scope = iota
	Project
	Local
)

type Catalog interface {
	Enabled(name string) bool
	Overrides(scope Scope) map[string]bool
	WithOverride(scope Scope, name string, enabled *bool) (Catalog, error)
}

type layeredCatalog struct {
	global  map[string]bool
	project map[string]bool
	local   map[string]bool
}

// New returns an immutable view of explicit per-scope overrides. A missing
// value is enabled, while later scopes override earlier scopes by name.
func New(global, project, local map[string]bool) Catalog {
	return layeredCatalog{global: clone(global), project: clone(project), local: clone(local)}
}

func (catalog layeredCatalog) Enabled(name string) bool {
	enabled := true
	for _, layer := range []map[string]bool{catalog.global, catalog.project, catalog.local} {
		if value, exists := layer[name]; exists {
			enabled = value
		}
	}
	return enabled
}

func (catalog layeredCatalog) Overrides(scope Scope) map[string]bool {
	var layer map[string]bool
	switch scope {
	case Global:
		layer = catalog.global
	case Project:
		layer = catalog.project
	case Local:
		layer = catalog.local
	default:
		return map[string]bool{}
	}
	return clone(layer)
}

func (catalog layeredCatalog) WithOverride(scope Scope, name string, enabled *bool) (Catalog, error) {
	if err := ValidateName(name); err != nil {
		return nil, err
	}
	updated := layeredCatalog{global: clone(catalog.global), project: clone(catalog.project), local: clone(catalog.local)}
	var target map[string]bool
	switch scope {
	case Global:
		target = updated.global
	case Project:
		target = updated.project
	case Local:
		target = updated.local
	default:
		return nil, errors.New("unknown capability scope")
	}
	if enabled == nil {
		delete(target, name)
	} else {
		target[name] = *enabled
	}
	return updated, nil
}

func ValidateName(name string) error {
	if strings.TrimSpace(name) == "" {
		return errors.New("capability name cannot be empty")
	}
	return nil
}

func clone(source map[string]bool) map[string]bool {
	result := make(map[string]bool, len(source))
	for name, enabled := range source {
		result[name] = enabled
	}
	return result
}
