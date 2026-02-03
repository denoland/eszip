// Copyright 2018-2024 the Deno authors. All rights reserved. MIT license.

package eszip

import (
	"context"
	"encoding/json"
	"net/url"
	"sync"
)

const eszipV1GraphVersion uint32 = 1

// EszipV1 represents a V1 eszip archive (JSON format)
type EszipV1 struct {
	Version uint32                       `json:"version"`
	Modules map[string]json.RawMessage   `json:"modules"`

	// Internal parsed modules
	mu            sync.RWMutex
	parsedModules map[string]*moduleInfoV1
}

// moduleInfoV1 represents a module in V1 format
type moduleInfoV1 struct {
	isRedirect bool
	redirect   string
	source     *moduleSourceV1
}

// moduleSourceV1 represents module source data
type moduleSourceV1 struct {
	Source      string   `json:"source"`
	Transpiled  *string  `json:"transpiled"`
	ContentType *string  `json:"content_type"`
	Deps        []string `json:"deps"`
}

// v1ModuleInfoJSON is used for JSON unmarshaling
type v1ModuleInfoJSON struct {
	Redirect *string          `json:"Redirect"`
	Source   *moduleSourceV1  `json:"Source"`
}

// ParseV1 parses a V1 eszip from JSON data
func ParseV1(data []byte) (*EszipV1, error) {
	var eszip EszipV1
	if err := json.Unmarshal(data, &eszip); err != nil {
		return nil, errInvalidV1Json(err)
	}

	if eszip.Version != eszipV1GraphVersion {
		return nil, errInvalidV1Version(eszip.Version)
	}

	// Parse all modules
	eszip.parsedModules = make(map[string]*moduleInfoV1)
	for specifier, raw := range eszip.Modules {
		var info v1ModuleInfoJSON
		if err := json.Unmarshal(raw, &info); err != nil {
			return nil, errInvalidV1Json(err)
		}

		moduleInfo := &moduleInfoV1{}
		if info.Redirect != nil {
			moduleInfo.isRedirect = true
			moduleInfo.redirect = *info.Redirect
		} else if info.Source != nil {
			moduleInfo.source = info.Source
		}
		eszip.parsedModules[specifier] = moduleInfo
	}

	return &eszip, nil
}

// GetModule returns the module for the given specifier, following redirects
func (e *EszipV1) GetModule(specifier string) *Module {
	// Parse URL to normalize it
	u, err := url.Parse(specifier)
	if err != nil {
		return nil
	}
	normalizedSpecifier := u.String()

	visited := make(map[string]bool)
	current := normalizedSpecifier

	e.mu.RLock()
	defer e.mu.RUnlock()

	for {
		if visited[current] {
			return nil // Cycle detected
		}
		visited[current] = true

		info, ok := e.parsedModules[current]
		if !ok {
			return nil
		}

		if info.isRedirect {
			current = info.redirect
			continue
		}

		return &Module{
			Specifier: current,
			Kind:      ModuleKindJavaScript,
			inner:     &v1ModuleInner{eszip: e},
		}
	}
}

// GetImportMap returns nil for V1 (V1 never contains import maps)
func (e *EszipV1) GetImportMap(specifier string) *Module {
	return nil
}

// Specifiers returns all module specifiers
func (e *EszipV1) Specifiers() []string {
	e.mu.RLock()
	defer e.mu.RUnlock()

	specs := make([]string, 0, len(e.parsedModules))
	for spec := range e.parsedModules {
		specs = append(specs, spec)
	}
	return specs
}

// IntoBytes serializes the V1 eszip to JSON
func (e *EszipV1) IntoBytes() ([]byte, error) {
	return json.Marshal(e)
}

// v1ModuleInner implements moduleInner for V1
type v1ModuleInner struct {
	eszip *EszipV1
}

func (v *v1ModuleInner) getSource(ctx context.Context, specifier string) ([]byte, error) {
	v.eszip.mu.RLock()
	defer v.eszip.mu.RUnlock()

	info, ok := v.eszip.parsedModules[specifier]
	if !ok || info.isRedirect || info.source == nil {
		return nil, nil
	}

	// Return transpiled if available, otherwise source
	if info.source.Transpiled != nil {
		return []byte(*info.source.Transpiled), nil
	}
	return []byte(info.source.Source), nil
}

func (v *v1ModuleInner) takeSource(ctx context.Context, specifier string) ([]byte, error) {
	v.eszip.mu.Lock()
	defer v.eszip.mu.Unlock()

	info, ok := v.eszip.parsedModules[specifier]
	if !ok || info.isRedirect || info.source == nil {
		return nil, nil
	}

	// Get the source
	var source []byte
	if info.source.Transpiled != nil {
		source = []byte(*info.source.Transpiled)
	} else {
		source = []byte(info.source.Source)
	}

	// Remove the module from the map (V1 behavior)
	delete(v.eszip.parsedModules, specifier)

	return source, nil
}

func (v *v1ModuleInner) getSourceMap(ctx context.Context, specifier string) ([]byte, error) {
	// V1 does not support source maps
	return nil, nil
}

func (v *v1ModuleInner) takeSourceMap(ctx context.Context, specifier string) ([]byte, error) {
	// V1 does not support source maps
	return nil, nil
}

// Iterate returns all modules as an iterator
func (e *EszipV1) Iterate() []struct {
	Specifier string
	Module    *Module
} {
	specs := e.Specifiers()
	result := make([]struct {
		Specifier string
		Module    *Module
	}, 0, len(specs))

	for _, spec := range specs {
		module := e.GetModule(spec)
		if module != nil {
			result = append(result, struct {
				Specifier string
				Module    *Module
			}{Specifier: spec, Module: module})
		}
	}

	return result
}
