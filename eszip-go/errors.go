// Copyright 2018-2024 the Deno authors. All rights reserved. MIT license.

package eszip

import "fmt"

// ParseErrorType represents the type of parse error
type ParseErrorType int

const (
	ErrInvalidV1Json ParseErrorType = iota
	ErrInvalidV1Version
	ErrInvalidV2
	ErrInvalidV2HeaderHash
	ErrInvalidV2Specifier
	ErrInvalidV2EntryKind
	ErrInvalidV2ModuleKind
	ErrInvalidV2Header
	ErrInvalidV2SourceOffset
	ErrInvalidV2SourceHash
	ErrInvalidV2NpmSnapshotHash
	ErrInvalidV2NpmPackageOffset
	ErrInvalidV2NpmPackage
	ErrInvalidV2NpmPackageReq
	ErrInvalidV22OptionsHeader
	ErrInvalidV22OptionsHeaderHash
	ErrIO
)

// ParseError represents an error that occurred during parsing
type ParseError struct {
	Type    ParseErrorType
	Message string
	Offset  int
}

func (e *ParseError) Error() string {
	if e.Offset > 0 {
		return fmt.Sprintf("eszip parse error: %s at offset %d", e.Message, e.Offset)
	}
	return fmt.Sprintf("eszip parse error: %s", e.Message)
}

// Error constructors for common parse errors

func errInvalidV1Json(err error) *ParseError {
	return &ParseError{Type: ErrInvalidV1Json, Message: fmt.Sprintf("invalid eszip v1 json: %v", err)}
}

func errInvalidV1Version(version uint32) *ParseError {
	return &ParseError{Type: ErrInvalidV1Version, Message: fmt.Sprintf("invalid eszip v1 version: got %d, expected 1", version)}
}

func errInvalidV2() *ParseError {
	return &ParseError{Type: ErrInvalidV2, Message: "invalid eszip v2"}
}

func errInvalidV2HeaderHash() *ParseError {
	return &ParseError{Type: ErrInvalidV2HeaderHash, Message: "invalid eszip v2 header hash"}
}

func errInvalidV2Specifier(offset int) *ParseError {
	return &ParseError{Type: ErrInvalidV2Specifier, Message: "invalid specifier in eszip v2 header", Offset: offset}
}

func errInvalidV2EntryKind(kind uint8, offset int) *ParseError {
	return &ParseError{Type: ErrInvalidV2EntryKind, Message: fmt.Sprintf("invalid entry kind %d in eszip v2 header", kind), Offset: offset}
}

func errInvalidV2ModuleKind(kind uint8, offset int) *ParseError {
	return &ParseError{Type: ErrInvalidV2ModuleKind, Message: fmt.Sprintf("invalid module kind %d in eszip v2 header", kind), Offset: offset}
}

func errInvalidV2Header(msg string) *ParseError {
	return &ParseError{Type: ErrInvalidV2Header, Message: fmt.Sprintf("invalid eszip v2 header: %s", msg)}
}

func errInvalidV2SourceOffset(offset int) *ParseError {
	return &ParseError{Type: ErrInvalidV2SourceOffset, Message: fmt.Sprintf("invalid eszip v2 source offset (%d)", offset), Offset: offset}
}

func errInvalidV2SourceHash(specifier string) *ParseError {
	return &ParseError{Type: ErrInvalidV2SourceHash, Message: fmt.Sprintf("invalid eszip v2 source hash (specifier %s)", specifier)}
}

func errInvalidV2NpmSnapshotHash() *ParseError {
	return &ParseError{Type: ErrInvalidV2NpmSnapshotHash, Message: "invalid eszip v2.1 npm snapshot hash"}
}

func errInvalidV2NpmPackageOffset(index int, err error) *ParseError {
	return &ParseError{Type: ErrInvalidV2NpmPackageOffset, Message: fmt.Sprintf("invalid eszip v2.1 npm package at index %d: %v", index, err)}
}

func errInvalidV2NpmPackage(name string, err error) *ParseError {
	return &ParseError{Type: ErrInvalidV2NpmPackage, Message: fmt.Sprintf("invalid eszip v2.1 npm package '%s': %v", name, err)}
}

func errInvalidV2NpmPackageReq(req string, err error) *ParseError {
	return &ParseError{Type: ErrInvalidV2NpmPackageReq, Message: fmt.Sprintf("invalid eszip v2.1 npm req '%s': %v", req, err)}
}

func errInvalidV22OptionsHeader(msg string) *ParseError {
	return &ParseError{Type: ErrInvalidV22OptionsHeader, Message: fmt.Sprintf("invalid eszip v2.2 options header: %s", msg)}
}

func errInvalidV22OptionsHeaderHash() *ParseError {
	return &ParseError{Type: ErrInvalidV22OptionsHeaderHash, Message: "invalid eszip v2.2 options header hash"}
}

func errIO(err error) *ParseError {
	return &ParseError{Type: ErrIO, Message: fmt.Sprintf("io error: %v", err)}
}
