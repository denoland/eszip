// Copyright 2018-2024 the Deno authors. All rights reserved. MIT license.

package eszip

import (
	"context"
	"sync"
)

// ModuleKind represents the type of module stored
type ModuleKind uint8

const (
	ModuleKindJavaScript ModuleKind = 0
	ModuleKindJson       ModuleKind = 1
	ModuleKindJsonc      ModuleKind = 2
	ModuleKindOpaqueData ModuleKind = 3
	ModuleKindWasm       ModuleKind = 4
)

func (k ModuleKind) String() string {
	switch k {
	case ModuleKindJavaScript:
		return "javascript"
	case ModuleKindJson:
		return "json"
	case ModuleKindJsonc:
		return "jsonc"
	case ModuleKindOpaqueData:
		return "opaque_data"
	case ModuleKindWasm:
		return "wasm"
	default:
		return "unknown"
	}
}

// Module represents a module in the eszip archive
type Module struct {
	Specifier string
	Kind      ModuleKind
	inner     moduleInner
}

// moduleInner provides access to module sources
type moduleInner interface {
	getSource(ctx context.Context, specifier string) ([]byte, error)
	takeSource(ctx context.Context, specifier string) ([]byte, error)
	getSourceMap(ctx context.Context, specifier string) ([]byte, error)
	takeSourceMap(ctx context.Context, specifier string) ([]byte, error)
}

// Source returns the source code of the module.
// This may block if the source hasn't been loaded yet (streaming).
func (m *Module) Source(ctx context.Context) ([]byte, error) {
	return m.inner.getSource(ctx, m.Specifier)
}

// TakeSource returns and removes the source from memory.
func (m *Module) TakeSource(ctx context.Context) ([]byte, error) {
	return m.inner.takeSource(ctx, m.Specifier)
}

// SourceMap returns the source map of the module (V2 only).
func (m *Module) SourceMap(ctx context.Context) ([]byte, error) {
	return m.inner.getSourceMap(ctx, m.Specifier)
}

// TakeSourceMap returns and removes the source map from memory.
func (m *Module) TakeSourceMap(ctx context.Context) ([]byte, error) {
	return m.inner.takeSourceMap(ctx, m.Specifier)
}

// SourceSlotState represents the state of a source slot
type SourceSlotState int

const (
	SourceSlotPending SourceSlotState = iota
	SourceSlotReady
	SourceSlotTaken
)

// SourceSlot represents a pending or loaded source
type SourceSlot struct {
	mu     sync.RWMutex
	state  SourceSlotState
	data   []byte
	offset uint32
	length uint32
	waitCh chan struct{}
}

// NewPendingSourceSlot creates a new pending source slot
func NewPendingSourceSlot(offset, length uint32) *SourceSlot {
	return &SourceSlot{
		state:  SourceSlotPending,
		offset: offset,
		length: length,
		waitCh: make(chan struct{}),
	}
}

// NewReadySourceSlot creates a new ready source slot with data
func NewReadySourceSlot(data []byte) *SourceSlot {
	ch := make(chan struct{})
	close(ch)
	return &SourceSlot{
		state:  SourceSlotReady,
		data:   data,
		waitCh: ch,
	}
}

// NewEmptySourceSlot creates a new ready source slot with empty data
func NewEmptySourceSlot() *SourceSlot {
	return NewReadySourceSlot([]byte{})
}

// SetReady marks the slot as ready with the given data
func (s *SourceSlot) SetReady(data []byte) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.data = data
	s.state = SourceSlotReady
	close(s.waitCh)
}

// Get returns the source data, blocking until ready or context cancelled
func (s *SourceSlot) Get(ctx context.Context) ([]byte, error) {
	s.mu.RLock()
	if s.state == SourceSlotReady {
		data := s.data
		s.mu.RUnlock()
		return data, nil
	}
	if s.state == SourceSlotTaken {
		s.mu.RUnlock()
		return nil, nil
	}
	waitCh := s.waitCh
	s.mu.RUnlock()

	select {
	case <-ctx.Done():
		return nil, ctx.Err()
	case <-waitCh:
		s.mu.RLock()
		defer s.mu.RUnlock()
		if s.state == SourceSlotTaken {
			return nil, nil
		}
		return s.data, nil
	}
}

// Take returns and removes the source data
func (s *SourceSlot) Take(ctx context.Context) ([]byte, error) {
	s.mu.RLock()
	if s.state == SourceSlotTaken {
		s.mu.RUnlock()
		return nil, nil
	}
	if s.state == SourceSlotPending {
		waitCh := s.waitCh
		s.mu.RUnlock()
		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		case <-waitCh:
		}
	} else {
		s.mu.RUnlock()
	}

	s.mu.Lock()
	defer s.mu.Unlock()
	if s.state == SourceSlotTaken {
		return nil, nil
	}
	data := s.data
	s.data = nil
	s.state = SourceSlotTaken
	return data, nil
}

// State returns the current state
func (s *SourceSlot) State() SourceSlotState {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.state
}

// Offset returns the offset in the sources section
func (s *SourceSlot) Offset() uint32 {
	return s.offset
}

// Length returns the length in the sources section
func (s *SourceSlot) Length() uint32 {
	return s.length
}
