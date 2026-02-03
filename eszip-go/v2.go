// Copyright 2018-2024 the Deno authors. All rights reserved. MIT license.

package eszip

import (
	"context"
	"sync"
)

// Magic bytes for V2 versions
var (
	MagicV2   = [8]byte{'E', 'S', 'Z', 'I', 'P', '_', 'V', '2'}
	MagicV2_1 = [8]byte{'E', 'S', 'Z', 'I', 'P', '2', '.', '1'}
	MagicV2_2 = [8]byte{'E', 'S', 'Z', 'I', 'P', '2', '.', '2'}
	MagicV2_3 = [8]byte{'E', 'S', 'Z', 'I', 'P', '2', '.', '3'}
)

// EszipVersion represents the V2 version
type EszipVersion int

const (
	VersionV2   EszipVersion = 0
	VersionV2_1 EszipVersion = 1
	VersionV2_2 EszipVersion = 2
	VersionV2_3 EszipVersion = 3
)

// LatestVersion is the latest supported version
const LatestVersion = VersionV2_3

// VersionFromMagic returns the version from magic bytes
func VersionFromMagic(magic []byte) (EszipVersion, bool) {
	if len(magic) < 8 {
		return 0, false
	}
	var m [8]byte
	copy(m[:], magic[:8])

	switch m {
	case MagicV2:
		return VersionV2, true
	case MagicV2_1:
		return VersionV2_1, true
	case MagicV2_2:
		return VersionV2_2, true
	case MagicV2_3:
		return VersionV2_3, true
	default:
		return 0, false
	}
}

// ToMagic returns the magic bytes for the version
func (v EszipVersion) ToMagic() [8]byte {
	switch v {
	case VersionV2:
		return MagicV2
	case VersionV2_1:
		return MagicV2_1
	case VersionV2_2:
		return MagicV2_2
	case VersionV2_3:
		return MagicV2_3
	default:
		return MagicV2_3
	}
}

// SupportsNpm returns true if the version supports npm
func (v EszipVersion) SupportsNpm() bool {
	return v != VersionV2
}

// SupportsOptions returns true if the version supports options header
func (v EszipVersion) SupportsOptions() bool {
	return v >= VersionV2_2
}

// HeaderFrameKind represents the type of entry in the modules header
type HeaderFrameKind uint8

const (
	HeaderFrameModule       HeaderFrameKind = 0
	HeaderFrameRedirect     HeaderFrameKind = 1
	HeaderFrameNpmSpecifier HeaderFrameKind = 2
)

// Options represents V2 options
type Options struct {
	Checksum     ChecksumType
	ChecksumSize uint8
}

// DefaultOptionsForVersion returns the default options for a version
func DefaultOptionsForVersion(version EszipVersion) Options {
	opts := Options{
		Checksum: ChecksumNone,
	}
	// Versions prior to v2.2 default to SHA256
	if version == VersionV2 || version == VersionV2_1 {
		opts.Checksum = ChecksumSha256
	}
	opts.ChecksumSize = opts.Checksum.DigestSize()
	return opts
}

// GetChecksumSize returns the effective checksum size
func (o Options) GetChecksumSize() uint8 {
	if o.ChecksumSize > 0 {
		return o.ChecksumSize
	}
	return o.Checksum.DigestSize()
}

// EszipV2 represents a V2 eszip archive
type EszipV2 struct {
	modules     *ModuleMap
	npmSnapshot *NpmResolutionSnapshot
	options     Options
	version     EszipVersion
}

// NewEszipV2 creates a new empty V2 eszip
func NewEszipV2() *EszipV2 {
	return &EszipV2{
		modules: NewModuleMap(),
		options: DefaultOptionsForVersion(LatestVersion),
		version: LatestVersion,
	}
}

// HasMagic checks if the buffer starts with a V2 magic
func HasMagic(buffer []byte) bool {
	if len(buffer) < 8 {
		return false
	}
	_, ok := VersionFromMagic(buffer[:8])
	return ok
}

// GetModule returns the module for the given specifier, following redirects
func (e *EszipV2) GetModule(specifier string) *Module {
	return e.getModuleInternal(specifier, false)
}

// GetImportMap returns the import map module for the given specifier
func (e *EszipV2) GetImportMap(specifier string) *Module {
	return e.getModuleInternal(specifier, true)
}

func (e *EszipV2) getModuleInternal(specifier string, allowJsonc bool) *Module {
	visited := make(map[string]bool)
	current := specifier

	for {
		if visited[current] {
			return nil // Cycle detected
		}
		visited[current] = true

		mod, ok := e.modules.Get(current)
		if !ok {
			return nil
		}

		switch m := mod.(type) {
		case *ModuleData:
			if m.Kind == ModuleKindJsonc && !allowJsonc {
				return nil
			}
			return &Module{
				Specifier: current,
				Kind:      m.Kind,
				inner:     &v2ModuleInner{eszip: e},
			}
		case *ModuleRedirect:
			current = m.Target
		case *NpmSpecifierEntry:
			// NPM specifiers are not regular modules
			return nil
		default:
			return nil
		}
	}
}

// Specifiers returns all module specifiers
func (e *EszipV2) Specifiers() []string {
	return e.modules.Keys()
}

// TakeNpmSnapshot removes and returns the NPM snapshot
func (e *EszipV2) TakeNpmSnapshot() *NpmResolutionSnapshot {
	snapshot := e.npmSnapshot
	e.npmSnapshot = nil
	return snapshot
}

// SetChecksum sets the checksum algorithm
func (e *EszipV2) SetChecksum(checksum ChecksumType) {
	e.options.Checksum = checksum
	e.options.ChecksumSize = checksum.DigestSize()
}

// AddModule adds a module to the archive
func (e *EszipV2) AddModule(specifier string, kind ModuleKind, source, sourceMap []byte) {
	e.modules.Insert(specifier, &ModuleData{
		Kind:      kind,
		Source:    NewReadySourceSlot(source),
		SourceMap: NewReadySourceSlot(sourceMap),
	})
}

// AddImportMap adds an import map at the front of the archive
func (e *EszipV2) AddImportMap(kind ModuleKind, specifier string, source []byte) {
	e.modules.InsertFront(specifier, &ModuleData{
		Kind:      kind,
		Source:    NewReadySourceSlot(source),
		SourceMap: NewEmptySourceSlot(),
	})
}

// AddRedirect adds a redirect entry
func (e *EszipV2) AddRedirect(specifier, target string) {
	e.modules.Insert(specifier, &ModuleRedirect{Target: target})
}

// AddOpaqueData adds opaque data to the archive
func (e *EszipV2) AddOpaqueData(specifier string, data []byte) {
	e.AddModule(specifier, ModuleKindOpaqueData, data, nil)
}

// Iterate returns all modules
func (e *EszipV2) Iterate() []struct {
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

// v2ModuleInner implements moduleInner for V2
type v2ModuleInner struct {
	eszip *EszipV2
}

func (v *v2ModuleInner) getSource(ctx context.Context, specifier string) ([]byte, error) {
	mod, ok := v.eszip.modules.Get(specifier)
	if !ok {
		return nil, nil
	}

	data, ok := mod.(*ModuleData)
	if !ok {
		return nil, nil
	}

	return data.Source.Get(ctx)
}

func (v *v2ModuleInner) takeSource(ctx context.Context, specifier string) ([]byte, error) {
	mod, ok := v.eszip.modules.Get(specifier)
	if !ok {
		return nil, nil
	}

	data, ok := mod.(*ModuleData)
	if !ok {
		return nil, nil
	}

	return data.Source.Take(ctx)
}

func (v *v2ModuleInner) getSourceMap(ctx context.Context, specifier string) ([]byte, error) {
	mod, ok := v.eszip.modules.Get(specifier)
	if !ok {
		return nil, nil
	}

	data, ok := mod.(*ModuleData)
	if !ok {
		return nil, nil
	}

	return data.SourceMap.Get(ctx)
}

func (v *v2ModuleInner) takeSourceMap(ctx context.Context, specifier string) ([]byte, error) {
	mod, ok := v.eszip.modules.Get(specifier)
	if !ok {
		return nil, nil
	}

	data, ok := mod.(*ModuleData)
	if !ok {
		return nil, nil
	}

	return data.SourceMap.Take(ctx)
}

// Section represents a parsed section with content and hash
type Section struct {
	content  []byte
	hash     []byte
	checksum ChecksumType
}

// Content returns the section content
func (s *Section) Content() []byte {
	return s.content
}

// ContentLen returns the content length
func (s *Section) ContentLen() int {
	return len(s.content)
}

// TotalLen returns the total length including hash
func (s *Section) TotalLen() int {
	return len(s.content) + len(s.hash)
}

// IsChecksumValid verifies the section checksum
func (s *Section) IsChecksumValid() bool {
	if s.checksum == ChecksumNone {
		return true
	}
	return s.checksum.Verify(s.content, s.hash)
}

// IntoContent returns and takes ownership of the content
func (s *Section) IntoContent() []byte {
	content := s.content
	s.content = nil
	return content
}

// NpmPackageIndex represents an npm package index
type NpmPackageIndex struct {
	Index uint32
}

// parserState holds state during parsing
type parserState struct {
	mu             sync.Mutex
	sourceOffsets  map[int]sourceOffsetEntry
	sourceMapOffsets map[int]sourceOffsetEntry
}

type sourceOffsetEntry struct {
	length    int
	specifier string
}
