// Copyright 2018-2024 the Deno authors. All rights reserved. MIT license.

package eszip

import (
	"bufio"
	"encoding/binary"
	"fmt"
	"strings"
)

// NpmResolutionSnapshot represents the NPM package resolution
type NpmResolutionSnapshot struct {
	Packages     []*NpmPackage
	RootPackages map[string]*NpmPackageID // req -> id
}

// NpmPackage represents a resolved NPM package
type NpmPackage struct {
	ID           *NpmPackageID
	Dependencies map[string]*NpmPackageID // req -> id
}

// NpmPackageID represents an NPM package identifier (name@version)
type NpmPackageID struct {
	Name    string
	Version string
}

// String returns the serialized form of the package ID
func (id *NpmPackageID) String() string {
	return fmt.Sprintf("%s@%s", id.Name, id.Version)
}

// ParseNpmPackageID parses a serialized NPM package ID (name@version)
func ParseNpmPackageID(s string) (*NpmPackageID, error) {
	// Find the last @ which separates name from version
	// Package names can contain @ (like @types/node)
	lastAt := strings.LastIndex(s, "@")
	if lastAt <= 0 {
		return nil, fmt.Errorf("invalid npm package id: %s", s)
	}

	return &NpmPackageID{
		Name:    s[:lastAt],
		Version: s[lastAt+1:],
	}, nil
}

// parseNpmSection parses the NPM section
func parseNpmSection(br *bufio.Reader, options Options, npmSpecifiers map[string]NpmPackageIndex) (*NpmResolutionSnapshot, error) {
	section, err := readSection(br, options)
	if err != nil {
		return nil, err
	}

	if !section.IsChecksumValid() {
		return nil, errInvalidV2NpmSnapshotHash()
	}

	content := section.Content()
	if len(content) == 0 {
		return nil, nil
	}

	// Parse packages
	packages := make([]*npmModuleEntry, 0)
	offset := 0

	for offset < len(content) {
		entry, newOffset, err := parseNpmModule(content, offset)
		if err != nil {
			return nil, errInvalidV2NpmPackageOffset(offset, err)
		}
		packages = append(packages, entry)
		offset = newOffset
	}

	// Build index to ID map
	pkgIndexToID := make(map[uint32]*NpmPackageID)
	for i, pkg := range packages {
		id, err := ParseNpmPackageID(pkg.name)
		if err != nil {
			return nil, errInvalidV2NpmPackage(pkg.name, err)
		}
		pkgIndexToID[uint32(i)] = id
	}

	// Build final packages
	finalPackages := make([]*NpmPackage, 0, len(packages))
	for i, pkg := range packages {
		id := pkgIndexToID[uint32(i)]
		deps := make(map[string]*NpmPackageID)

		for req, idx := range pkg.dependencies {
			depID, ok := pkgIndexToID[idx]
			if !ok {
				return nil, errInvalidV2NpmPackage(pkg.name, fmt.Errorf("missing index '%d'", idx))
			}
			deps[req] = depID
		}

		finalPackages = append(finalPackages, &NpmPackage{
			ID:           id,
			Dependencies: deps,
		})
	}

	// Build root packages
	rootPackages := make(map[string]*NpmPackageID)
	for req, idx := range npmSpecifiers {
		id, ok := pkgIndexToID[idx.Index]
		if !ok {
			return nil, errInvalidV2NpmPackageReq(req, fmt.Errorf("missing index '%d'", idx.Index))
		}
		rootPackages[req] = id
	}

	return &NpmResolutionSnapshot{
		Packages:     finalPackages,
		RootPackages: rootPackages,
	}, nil
}

// npmModuleEntry is an intermediate structure for parsing
type npmModuleEntry struct {
	name         string
	dependencies map[string]uint32 // req -> package index
}

func parseNpmModule(content []byte, offset int) (*npmModuleEntry, int, error) {
	// Parse name
	name, offset, err := parseNpmString(content, offset)
	if err != nil {
		return nil, 0, err
	}

	// Parse dependency count
	if offset+4 > len(content) {
		return nil, 0, fmt.Errorf("unexpected end of data")
	}
	depCount := binary.BigEndian.Uint32(content[offset : offset+4])
	offset += 4

	// Parse dependencies
	deps := make(map[string]uint32)
	for i := uint32(0); i < depCount; i++ {
		// Parse dependency name
		depName, newOffset, err := parseNpmString(content, offset)
		if err != nil {
			return nil, 0, err
		}
		offset = newOffset

		// Parse package index
		if offset+4 > len(content) {
			return nil, 0, fmt.Errorf("unexpected end of data")
		}
		pkgIndex := binary.BigEndian.Uint32(content[offset : offset+4])
		offset += 4

		deps[depName] = pkgIndex
	}

	return &npmModuleEntry{
		name:         name,
		dependencies: deps,
	}, offset, nil
}

func parseNpmString(content []byte, offset int) (string, int, error) {
	if offset+4 > len(content) {
		return "", 0, fmt.Errorf("unexpected end of data")
	}

	length := binary.BigEndian.Uint32(content[offset : offset+4])
	offset += 4

	if offset+int(length) > len(content) {
		return "", 0, fmt.Errorf("unexpected end of data")
	}

	str := string(content[offset : offset+int(length)])
	offset += int(length)

	return str, offset, nil
}
