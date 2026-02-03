// Copyright 2018-2024 the Deno authors. All rights reserved. MIT license.

package eszip

import (
	"bufio"
	"context"
	"encoding/binary"
	"io"
)

// ParseV2 parses a V2 eszip from a reader.
// Returns the eszip and a completion function that loads sources in background.
func ParseV2(ctx context.Context, r io.Reader) (*EszipV2, func(context.Context) error, error) {
	br := bufio.NewReader(r)

	// Read magic bytes
	magic := make([]byte, 8)
	if _, err := io.ReadFull(br, magic); err != nil {
		return nil, nil, errIO(err)
	}

	version, ok := VersionFromMagic(magic)
	if !ok {
		return nil, nil, errInvalidV2()
	}

	return parseV2WithVersion(ctx, version, br)
}

// ParseV2Sync parses a V2 eszip completely (blocking)
func ParseV2Sync(ctx context.Context, r io.Reader) (*EszipV2, error) {
	eszip, complete, err := ParseV2(ctx, r)
	if err != nil {
		return nil, err
	}

	if err := complete(ctx); err != nil {
		return nil, err
	}

	return eszip, nil
}

func parseV2WithVersion(ctx context.Context, version EszipVersion, br *bufio.Reader) (*EszipV2, func(context.Context) error, error) {
	supportsNpm := version.SupportsNpm()
	supportsOptions := version.SupportsOptions()

	options := DefaultOptionsForVersion(version)

	// Parse options header (V2.2+)
	if supportsOptions {
		var err error
		options, err = parseOptionsHeader(br, options)
		if err != nil {
			return nil, nil, err
		}
	}

	// Parse modules header
	modulesHeader, err := readSection(br, options)
	if err != nil {
		return nil, nil, err
	}

	if !modulesHeader.IsChecksumValid() {
		return nil, nil, errInvalidV2HeaderHash()
	}

	// Parse module entries from header
	modules, npmSpecifiers, err := parseModulesHeader(modulesHeader.Content(), supportsNpm)
	if err != nil {
		return nil, nil, err
	}

	// Parse NPM section
	var npmSnapshot *NpmResolutionSnapshot
	if supportsNpm {
		npmSnapshot, err = parseNpmSection(br, options, npmSpecifiers)
		if err != nil {
			return nil, nil, err
		}
	}

	// Build source offset maps
	sourceOffsets := make(map[int]sourceOffsetEntry)
	sourceMapOffsets := make(map[int]sourceOffsetEntry)

	for _, specifier := range modules.Keys() {
		mod, ok := modules.Get(specifier)
		if !ok {
			continue
		}

		data, ok := mod.(*ModuleData)
		if !ok {
			continue
		}

		if data.Source.State() == SourceSlotPending && data.Source.Length() > 0 {
			sourceOffsets[int(data.Source.Offset())] = sourceOffsetEntry{
				length:    int(data.Source.Length()),
				specifier: specifier,
			}
		}

		if data.SourceMap.State() == SourceSlotPending && data.SourceMap.Length() > 0 {
			sourceMapOffsets[int(data.SourceMap.Offset())] = sourceOffsetEntry{
				length:    int(data.SourceMap.Length()),
				specifier: specifier,
			}
		}
	}

	eszip := &EszipV2{
		modules:     modules,
		npmSnapshot: npmSnapshot,
		options:     options,
		version:     version,
	}

	// Return completion function for source loading
	completeFn := func(ctx context.Context) error {
		return loadSources(ctx, br, eszip, options, sourceOffsets, sourceMapOffsets)
	}

	return eszip, completeFn, nil
}

func parseOptionsHeader(br *bufio.Reader, defaults Options) (Options, error) {
	// Read options without checksum first
	preOpts := defaults
	preOpts.Checksum = ChecksumNone
	preOpts.ChecksumSize = 0

	optionsHeader, err := readSection(br, preOpts)
	if err != nil {
		return defaults, err
	}

	if optionsHeader.ContentLen()%2 != 0 {
		return defaults, errInvalidV22OptionsHeader("options are expected to be byte tuples")
	}

	options := defaults
	content := optionsHeader.Content()

	for i := 0; i < len(content); i += 2 {
		option := content[i]
		value := content[i+1]

		switch option {
		case 0: // Checksum type
			checksum, ok := ChecksumFromU8(value)
			if ok {
				options.Checksum = checksum
			}
		case 1: // Checksum size
			options.ChecksumSize = value
		}
		// Unknown options are ignored for forward compatibility
	}

	if options.GetChecksumSize() == 0 && options.Checksum != ChecksumNone {
		return defaults, errInvalidV22OptionsHeader("checksum size must be known")
	}

	// If checksum is enabled, validate the options header hash
	if options.GetChecksumSize() > 0 {
		// Read the hash that follows
		hash := make([]byte, options.GetChecksumSize())
		if _, err := io.ReadFull(br, hash); err != nil {
			return defaults, errIO(err)
		}

		if !options.Checksum.Verify(content, hash) {
			return defaults, errInvalidV22OptionsHeaderHash()
		}
	}

	return options, nil
}

func readSection(br *bufio.Reader, options Options) (*Section, error) {
	// Read length (4 bytes, big-endian)
	lengthBytes := make([]byte, 4)
	if _, err := io.ReadFull(br, lengthBytes); err != nil {
		return nil, errIO(err)
	}
	length := binary.BigEndian.Uint32(lengthBytes)

	// Read content
	content := make([]byte, length)
	if _, err := io.ReadFull(br, content); err != nil {
		return nil, errIO(err)
	}

	// Read hash
	checksumSize := options.GetChecksumSize()
	var hash []byte
	if checksumSize > 0 {
		hash = make([]byte, checksumSize)
		if _, err := io.ReadFull(br, hash); err != nil {
			return nil, errIO(err)
		}
	}

	return &Section{
		content:  content,
		hash:     hash,
		checksum: options.Checksum,
	}, nil
}

func readSectionWithSize(br *bufio.Reader, options Options, contentLen int) (*Section, error) {
	// Read content
	content := make([]byte, contentLen)
	if _, err := io.ReadFull(br, content); err != nil {
		return nil, errIO(err)
	}

	// Read hash
	checksumSize := options.GetChecksumSize()
	var hash []byte
	if checksumSize > 0 {
		hash = make([]byte, checksumSize)
		if _, err := io.ReadFull(br, hash); err != nil {
			return nil, errIO(err)
		}
	}

	return &Section{
		content:  content,
		hash:     hash,
		checksum: options.Checksum,
	}, nil
}

func parseModulesHeader(content []byte, supportsNpm bool) (*ModuleMap, map[string]NpmPackageIndex, error) {
	modules := NewModuleMap()
	npmSpecifiers := make(map[string]NpmPackageIndex)

	read := 0

	for read < len(content) {
		// Read specifier length
		if read+4 > len(content) {
			return nil, nil, errInvalidV2Header("specifier len")
		}
		specifierLen := int(binary.BigEndian.Uint32(content[read : read+4]))
		read += 4

		// Read specifier
		if read+specifierLen > len(content) {
			return nil, nil, errInvalidV2Header("specifier")
		}
		specifier := string(content[read : read+specifierLen])
		read += specifierLen

		// Read entry kind
		if read+1 > len(content) {
			return nil, nil, errInvalidV2Header("entry kind")
		}
		entryKind := content[read]
		read++

		switch entryKind {
		case 0: // Module
			if read+17 > len(content) {
				return nil, nil, errInvalidV2Header("module data")
			}

			sourceOffset := binary.BigEndian.Uint32(content[read : read+4])
			read += 4
			sourceLen := binary.BigEndian.Uint32(content[read : read+4])
			read += 4
			sourceMapOffset := binary.BigEndian.Uint32(content[read : read+4])
			read += 4
			sourceMapLen := binary.BigEndian.Uint32(content[read : read+4])
			read += 4
			kindByte := content[read]
			read++

			var kind ModuleKind
			switch kindByte {
			case 0:
				kind = ModuleKindJavaScript
			case 1:
				kind = ModuleKindJson
			case 2:
				kind = ModuleKindJsonc
			case 3:
				kind = ModuleKindOpaqueData
			case 4:
				kind = ModuleKindWasm
			default:
				return nil, nil, errInvalidV2ModuleKind(kindByte, read)
			}

			var source *SourceSlot
			if sourceOffset == 0 && sourceLen == 0 {
				source = NewEmptySourceSlot()
			} else {
				source = NewPendingSourceSlot(sourceOffset, sourceLen)
			}

			var sourceMap *SourceSlot
			if sourceMapOffset == 0 && sourceMapLen == 0 {
				sourceMap = NewEmptySourceSlot()
			} else {
				sourceMap = NewPendingSourceSlot(sourceMapOffset, sourceMapLen)
			}

			modules.Insert(specifier, &ModuleData{
				Kind:      kind,
				Source:    source,
				SourceMap: sourceMap,
			})

		case 1: // Redirect
			if read+4 > len(content) {
				return nil, nil, errInvalidV2Header("target len")
			}
			targetLen := int(binary.BigEndian.Uint32(content[read : read+4]))
			read += 4

			if read+targetLen > len(content) {
				return nil, nil, errInvalidV2Header("target")
			}
			target := string(content[read : read+targetLen])
			read += targetLen

			modules.Insert(specifier, &ModuleRedirect{Target: target})

		case 2: // NpmSpecifier
			if !supportsNpm {
				return nil, nil, errInvalidV2EntryKind(entryKind, read)
			}

			if read+4 > len(content) {
				return nil, nil, errInvalidV2Header("npm package id")
			}
			pkgID := binary.BigEndian.Uint32(content[read : read+4])
			read += 4

			npmSpecifiers[specifier] = NpmPackageIndex{Index: pkgID}

		default:
			return nil, nil, errInvalidV2EntryKind(entryKind, read)
		}
	}

	return modules, npmSpecifiers, nil
}

func loadSources(ctx context.Context, br *bufio.Reader, eszip *EszipV2, options Options, sourceOffsets, sourceMapOffsets map[int]sourceOffsetEntry) error {
	// Read sources section
	sourcesLenBytes := make([]byte, 4)
	if _, err := io.ReadFull(br, sourcesLenBytes); err != nil {
		return errIO(err)
	}
	sourcesLen := int(binary.BigEndian.Uint32(sourcesLenBytes))

	read := 0
	for read < sourcesLen {
		entry, ok := sourceOffsets[read]
		if !ok {
			return errInvalidV2SourceOffset(read)
		}

		section, err := readSectionWithSize(br, options, entry.length)
		if err != nil {
			return err
		}

		if !section.IsChecksumValid() {
			return errInvalidV2SourceHash(entry.specifier)
		}

		read += section.TotalLen()

		// Update the module's source slot
		mod, ok := eszip.modules.Get(entry.specifier)
		if !ok {
			continue
		}

		data, ok := mod.(*ModuleData)
		if !ok {
			continue
		}

		data.Source.SetReady(section.IntoContent())
	}

	// Read source maps section
	sourceMapsLenBytes := make([]byte, 4)
	if _, err := io.ReadFull(br, sourceMapsLenBytes); err != nil {
		return errIO(err)
	}
	sourceMapsLen := int(binary.BigEndian.Uint32(sourceMapsLenBytes))

	read = 0
	for read < sourceMapsLen {
		entry, ok := sourceMapOffsets[read]
		if !ok {
			return errInvalidV2SourceOffset(read)
		}

		section, err := readSectionWithSize(br, options, entry.length)
		if err != nil {
			return err
		}

		if !section.IsChecksumValid() {
			return errInvalidV2SourceHash(entry.specifier)
		}

		read += section.TotalLen()

		// Update the module's source map slot
		mod, ok := eszip.modules.Get(entry.specifier)
		if !ok {
			continue
		}

		data, ok := mod.(*ModuleData)
		if !ok {
			continue
		}

		data.SourceMap.SetReady(section.IntoContent())
	}

	return nil
}

func readU32(r io.Reader) (uint32, error) {
	buf := make([]byte, 4)
	if _, err := io.ReadFull(r, buf); err != nil {
		return 0, err
	}
	return binary.BigEndian.Uint32(buf), nil
}
