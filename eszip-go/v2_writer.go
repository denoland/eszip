// Copyright 2018-2024 the Deno authors. All rights reserved. MIT license.

package eszip

import (
	"encoding/binary"
	"sort"
)

// IntoBytes serializes the eszip archive to bytes
func (e *EszipV2) IntoBytes() ([]byte, error) {
	checksum := e.options.Checksum
	checksumSize := e.options.GetChecksumSize()

	var result []byte

	// Write magic (latest version)
	magic := LatestVersion.ToMagic()
	result = append(result, magic[:]...)

	// Build options header
	optionsHeaderContent := []byte{
		0, byte(checksum),     // Checksum type
		1, byte(checksumSize), // Checksum size
	}

	// Write options header length
	optionsHeaderLenBytes := make([]byte, 4)
	binary.BigEndian.PutUint32(optionsHeaderLenBytes, uint32(len(optionsHeaderContent)))
	result = append(result, optionsHeaderLenBytes...)

	// Write options header content
	result = append(result, optionsHeaderContent...)

	// Write options header hash
	optionsHash := checksum.Hash(optionsHeaderContent)
	result = append(result, optionsHash...)

	// Build modules header, sources, and source maps
	var modulesHeader []byte
	var sources []byte
	var sourceMaps []byte

	keys := e.modules.Keys()
	for _, specifier := range keys {
		mod, ok := e.modules.Get(specifier)
		if !ok {
			continue
		}

		// Write specifier
		appendString(&modulesHeader, specifier)

		switch m := mod.(type) {
		case *ModuleData:
			// Write module entry
			modulesHeader = append(modulesHeader, byte(HeaderFrameModule))

			// Get source bytes
			sourceBytes := m.Source.data
			if sourceBytes == nil && m.Source.State() == SourceSlotReady {
				sourceBytes = []byte{}
			}
			sourceLen := uint32(len(sourceBytes))

			if sourceLen > 0 {
				sourceOffset := uint32(len(sources))
				sources = append(sources, sourceBytes...)
				sources = append(sources, checksum.Hash(sourceBytes)...)

				modulesHeader = appendU32BE(modulesHeader, sourceOffset)
				modulesHeader = appendU32BE(modulesHeader, sourceLen)
			} else {
				modulesHeader = appendU32BE(modulesHeader, 0)
				modulesHeader = appendU32BE(modulesHeader, 0)
			}

			// Get source map bytes
			sourceMapBytes := m.SourceMap.data
			if sourceMapBytes == nil && m.SourceMap.State() == SourceSlotReady {
				sourceMapBytes = []byte{}
			}
			sourceMapLen := uint32(len(sourceMapBytes))

			if sourceMapLen > 0 {
				sourceMapOffset := uint32(len(sourceMaps))
				sourceMaps = append(sourceMaps, sourceMapBytes...)
				sourceMaps = append(sourceMaps, checksum.Hash(sourceMapBytes)...)

				modulesHeader = appendU32BE(modulesHeader, sourceMapOffset)
				modulesHeader = appendU32BE(modulesHeader, sourceMapLen)
			} else {
				modulesHeader = appendU32BE(modulesHeader, 0)
				modulesHeader = appendU32BE(modulesHeader, 0)
			}

			// Write module kind
			modulesHeader = append(modulesHeader, byte(m.Kind))

		case *ModuleRedirect:
			// Write redirect entry
			modulesHeader = append(modulesHeader, byte(HeaderFrameRedirect))
			appendString(&modulesHeader, m.Target)

		case *NpmSpecifierEntry:
			// Write npm specifier entry
			modulesHeader = append(modulesHeader, byte(HeaderFrameNpmSpecifier))
			modulesHeader = appendU32BE(modulesHeader, m.PackageID)
		}
	}

	// Add npm snapshot entries if present
	var npmBytes []byte
	if e.npmSnapshot != nil {
		// Sort packages by ID for determinism
		packages := make([]*NpmPackage, len(e.npmSnapshot.Packages))
		copy(packages, e.npmSnapshot.Packages)
		sort.Slice(packages, func(i, j int) bool {
			return packages[i].ID.String() < packages[j].ID.String()
		})

		// Build ID to index map
		idToIndex := make(map[string]uint32)
		for i, pkg := range packages {
			idToIndex[pkg.ID.String()] = uint32(i)
		}

		// Write root packages to modules header
		rootPkgs := make([]struct {
			req string
			id  string
		}, 0, len(e.npmSnapshot.RootPackages))
		for req, id := range e.npmSnapshot.RootPackages {
			rootPkgs = append(rootPkgs, struct {
				req string
				id  string
			}{req: req, id: id.String()})
		}
		sort.Slice(rootPkgs, func(i, j int) bool {
			return rootPkgs[i].req < rootPkgs[j].req
		})

		for _, rp := range rootPkgs {
			appendString(&modulesHeader, rp.req)
			modulesHeader = append(modulesHeader, byte(HeaderFrameNpmSpecifier))
			modulesHeader = appendU32BE(modulesHeader, idToIndex[rp.id])
		}

		// Write packages to npm bytes
		for _, pkg := range packages {
			appendString(&npmBytes, pkg.ID.String())

			// Write dependencies count
			npmBytes = appendU32BE(npmBytes, uint32(len(pkg.Dependencies)))

			// Sort dependencies for determinism
			deps := make([]struct {
				req string
				id  string
			}, 0, len(pkg.Dependencies))
			for req, id := range pkg.Dependencies {
				deps = append(deps, struct {
					req string
					id  string
				}{req: req, id: id.String()})
			}
			sort.Slice(deps, func(i, j int) bool {
				return deps[i].req < deps[j].req
			})

			for _, dep := range deps {
				appendString(&npmBytes, dep.req)
				npmBytes = appendU32BE(npmBytes, idToIndex[dep.id])
			}
		}
	}

	// Write modules header length
	modulesHeaderLenBytes := make([]byte, 4)
	binary.BigEndian.PutUint32(modulesHeaderLenBytes, uint32(len(modulesHeader)))
	result = append(result, modulesHeaderLenBytes...)

	// Write modules header content
	result = append(result, modulesHeader...)

	// Write modules header hash
	modulesHash := checksum.Hash(modulesHeader)
	result = append(result, modulesHash...)

	// Write npm section
	npmLenBytes := make([]byte, 4)
	binary.BigEndian.PutUint32(npmLenBytes, uint32(len(npmBytes)))
	result = append(result, npmLenBytes...)
	result = append(result, npmBytes...)
	result = append(result, checksum.Hash(npmBytes)...)

	// Write sources section
	sourcesLenBytes := make([]byte, 4)
	binary.BigEndian.PutUint32(sourcesLenBytes, uint32(len(sources)))
	result = append(result, sourcesLenBytes...)
	result = append(result, sources...)

	// Write source maps section
	sourceMapsLenBytes := make([]byte, 4)
	binary.BigEndian.PutUint32(sourceMapsLenBytes, uint32(len(sourceMaps)))
	result = append(result, sourceMapsLenBytes...)
	result = append(result, sourceMaps...)

	return result, nil
}

func appendString(buf *[]byte, s string) {
	lenBytes := make([]byte, 4)
	binary.BigEndian.PutUint32(lenBytes, uint32(len(s)))
	*buf = append(*buf, lenBytes...)
	*buf = append(*buf, []byte(s)...)
}

func appendU32BE(buf []byte, v uint32) []byte {
	b := make([]byte, 4)
	binary.BigEndian.PutUint32(b, v)
	return append(buf, b...)
}
