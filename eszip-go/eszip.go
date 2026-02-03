// Copyright 2018-2024 the Deno authors. All rights reserved. MIT license.

// Package eszip provides functionality for reading and writing eszip archives.
// Eszip is a binary serialization format for ECMAScript module graphs, used by Deno.
package eszip

import (
	"bufio"
	"context"
	"io"
)

// Eszip is the unified interface for V1 and V2 eszip archives
type Eszip interface {
	// GetModule returns the module for the given specifier, following redirects.
	// Returns nil if not found or if the module is JSONC (use GetImportMap instead).
	GetModule(specifier string) *Module

	// GetImportMap returns the import map module for the given specifier.
	// Unlike GetModule, this can return JSONC modules.
	GetImportMap(specifier string) *Module

	// Specifiers returns all module specifiers in the archive.
	Specifiers() []string

	// TakeNpmSnapshot removes and returns the NPM resolution snapshot.
	// Returns nil for V1 archives or if already taken.
	TakeNpmSnapshot() *NpmResolutionSnapshot
}

// EszipUnion wraps either V1 or V2 eszip
type EszipUnion struct {
	v1 *EszipV1
	v2 *EszipV2
}

// IsV1 returns true if this is a V1 archive
func (e *EszipUnion) IsV1() bool {
	return e.v1 != nil
}

// IsV2 returns true if this is a V2 archive
func (e *EszipUnion) IsV2() bool {
	return e.v2 != nil
}

// V1 returns the V1 archive (panics if not V1)
func (e *EszipUnion) V1() *EszipV1 {
	if e.v1 == nil {
		panic("not a V1 eszip")
	}
	return e.v1
}

// V2 returns the V2 archive (panics if not V2)
func (e *EszipUnion) V2() *EszipV2 {
	if e.v2 == nil {
		panic("not a V2 eszip")
	}
	return e.v2
}

// GetModule returns the module for the given specifier
func (e *EszipUnion) GetModule(specifier string) *Module {
	if e.v1 != nil {
		return e.v1.GetModule(specifier)
	}
	return e.v2.GetModule(specifier)
}

// GetImportMap returns the import map module for the given specifier
func (e *EszipUnion) GetImportMap(specifier string) *Module {
	if e.v1 != nil {
		return e.v1.GetImportMap(specifier)
	}
	return e.v2.GetImportMap(specifier)
}

// Specifiers returns all module specifiers
func (e *EszipUnion) Specifiers() []string {
	if e.v1 != nil {
		return e.v1.Specifiers()
	}
	return e.v2.Specifiers()
}

// TakeNpmSnapshot removes and returns the NPM snapshot
func (e *EszipUnion) TakeNpmSnapshot() *NpmResolutionSnapshot {
	if e.v1 != nil {
		return nil
	}
	return e.v2.TakeNpmSnapshot()
}

// Parse parses an eszip archive from the given reader.
// Returns the eszip and a function to complete parsing of source data (for streaming).
// The completion function must be called to fully load sources.
func Parse(ctx context.Context, r io.Reader) (*EszipUnion, func(context.Context) error, error) {
	br := bufio.NewReader(r)

	// Read magic bytes
	magic := make([]byte, 8)
	if _, err := io.ReadFull(br, magic); err != nil {
		return nil, nil, errIO(err)
	}

	// Check if it's V2
	if version, ok := VersionFromMagic(magic); ok {
		eszip, complete, err := parseV2WithVersion(ctx, version, br)
		if err != nil {
			return nil, nil, err
		}
		return &EszipUnion{v2: eszip}, complete, nil
	}

	// Otherwise, treat as V1 JSON - read the rest
	var allData []byte
	allData = append(allData, magic...)
	remaining, err := io.ReadAll(br)
	if err != nil {
		return nil, nil, errIO(err)
	}
	allData = append(allData, remaining...)

	eszip, err := ParseV1(allData)
	if err != nil {
		return nil, nil, err
	}

	// V1 has no streaming, completion is a no-op
	complete := func(ctx context.Context) error {
		return nil
	}

	return &EszipUnion{v1: eszip}, complete, nil
}

// ParseSync parses an eszip archive completely (blocking)
func ParseSync(ctx context.Context, r io.Reader) (*EszipUnion, error) {
	eszip, complete, err := Parse(ctx, r)
	if err != nil {
		return nil, err
	}

	if err := complete(ctx); err != nil {
		return nil, err
	}

	return eszip, nil
}

// ParseBytes parses an eszip from a byte slice
func ParseBytes(ctx context.Context, data []byte) (*EszipUnion, error) {
	return ParseSync(ctx, &byteReader{data: data})
}

// byteReader wraps a byte slice as an io.Reader
type byteReader struct {
	data   []byte
	offset int
}

func (r *byteReader) Read(p []byte) (n int, err error) {
	if r.offset >= len(r.data) {
		return 0, io.EOF
	}
	n = copy(p, r.data[r.offset:])
	r.offset += n
	return n, nil
}

// NewV2 creates a new empty V2 eszip archive
func NewV2() *EszipV2 {
	return NewEszipV2()
}
