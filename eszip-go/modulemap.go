// Copyright 2018-2024 the Deno authors. All rights reserved. MIT license.

package eszip

import (
	"sync"
)

// ModuleMap is a thread-safe ordered map of modules
type ModuleMap struct {
	mu    sync.RWMutex
	order []string
	data  map[string]EszipV2Module
}

// EszipV2Module represents a module entry in V2 format
type EszipV2Module interface {
	isEszipV2Module()
}

// ModuleData represents an actual module with source
type ModuleData struct {
	Kind      ModuleKind
	Source    *SourceSlot
	SourceMap *SourceSlot
}

func (ModuleData) isEszipV2Module() {}

// ModuleRedirect represents a redirect to another specifier
type ModuleRedirect struct {
	Target string
}

func (ModuleRedirect) isEszipV2Module() {}

// NpmSpecifierEntry represents an npm specifier entry
type NpmSpecifierEntry struct {
	PackageID uint32
}

func (NpmSpecifierEntry) isEszipV2Module() {}

// NewModuleMap creates a new module map
func NewModuleMap() *ModuleMap {
	return &ModuleMap{
		order: make([]string, 0),
		data:  make(map[string]EszipV2Module),
	}
}

// Insert adds or updates a module
func (m *ModuleMap) Insert(specifier string, module EszipV2Module) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if _, exists := m.data[specifier]; !exists {
		m.order = append(m.order, specifier)
	}
	m.data[specifier] = module
}

// InsertFront adds a module at the front (for import maps)
func (m *ModuleMap) InsertFront(specifier string, module EszipV2Module) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if _, exists := m.data[specifier]; exists {
		// Remove from current position
		for i, s := range m.order {
			if s == specifier {
				m.order = append(m.order[:i], m.order[i+1:]...)
				break
			}
		}
	}
	m.order = append([]string{specifier}, m.order...)
	m.data[specifier] = module
}

// Get retrieves a module
func (m *ModuleMap) Get(specifier string) (EszipV2Module, bool) {
	m.mu.RLock()
	defer m.mu.RUnlock()
	mod, ok := m.data[specifier]
	return mod, ok
}

// GetMut retrieves a module for mutation (returns the pointer)
func (m *ModuleMap) GetMut(specifier string) EszipV2Module {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.data[specifier]
}

// Remove removes a module and returns it
func (m *ModuleMap) Remove(specifier string) (EszipV2Module, bool) {
	m.mu.Lock()
	defer m.mu.Unlock()
	mod, ok := m.data[specifier]
	if ok {
		delete(m.data, specifier)
		for i, s := range m.order {
			if s == specifier {
				m.order = append(m.order[:i], m.order[i+1:]...)
				break
			}
		}
	}
	return mod, ok
}

// Keys returns all specifiers in order
func (m *ModuleMap) Keys() []string {
	m.mu.RLock()
	defer m.mu.RUnlock()
	keys := make([]string, len(m.order))
	copy(keys, m.order)
	return keys
}

// Len returns the number of modules
func (m *ModuleMap) Len() int {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return len(m.order)
}

// ModuleEntry represents a specifier-module pair for iteration
type ModuleEntry struct {
	Specifier string
	Module    EszipV2Module
}

// Iterate returns a channel that yields all modules
func (m *ModuleMap) Iterate() <-chan ModuleEntry {
	ch := make(chan ModuleEntry)
	go func() {
		defer close(ch)
		m.mu.RLock()
		keys := make([]string, len(m.order))
		copy(keys, m.order)
		m.mu.RUnlock()

		for _, key := range keys {
			m.mu.RLock()
			mod, ok := m.data[key]
			m.mu.RUnlock()
			if ok {
				ch <- ModuleEntry{Specifier: key, Module: mod}
			}
		}
	}()
	return ch
}
