// Copyright 2018-2024 the Deno authors. All rights reserved. MIT license.

package eszip

import (
	"bytes"
	"crypto/sha256"

	"github.com/zeebo/xxh3"
)

// ChecksumType represents the hash algorithm used for checksums
type ChecksumType uint8

const (
	ChecksumNone   ChecksumType = 0
	ChecksumSha256 ChecksumType = 1
	ChecksumXxh3   ChecksumType = 2
)

// DigestSize returns the size in bytes of the hash digest
func (c ChecksumType) DigestSize() uint8 {
	switch c {
	case ChecksumNone:
		return 0
	case ChecksumSha256:
		return 32
	case ChecksumXxh3:
		return 8
	default:
		return 0
	}
}

// Hash computes the checksum of the given data
func (c ChecksumType) Hash(data []byte) []byte {
	switch c {
	case ChecksumNone:
		return nil
	case ChecksumSha256:
		h := sha256.Sum256(data)
		return h[:]
	case ChecksumXxh3:
		h := xxh3.Hash(data)
		// Convert to big-endian bytes
		return []byte{
			byte(h >> 56),
			byte(h >> 48),
			byte(h >> 40),
			byte(h >> 32),
			byte(h >> 24),
			byte(h >> 16),
			byte(h >> 8),
			byte(h),
		}
	default:
		return nil
	}
}

// Verify checks if the given hash matches the data
func (c ChecksumType) Verify(data, hash []byte) bool {
	if c == ChecksumNone {
		return true
	}
	computed := c.Hash(data)
	return bytes.Equal(computed, hash)
}

// FromU8 creates a ChecksumType from a byte value
func ChecksumFromU8(b uint8) (ChecksumType, bool) {
	switch b {
	case 0:
		return ChecksumNone, true
	case 1:
		return ChecksumSha256, true
	case 2:
		return ChecksumXxh3, true
	default:
		return ChecksumNone, false
	}
}
