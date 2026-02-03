// Copyright 2018-2024 the Deno authors. All rights reserved. MIT license.

package eszip

import (
	"bytes"
	"context"
	"os"
	"testing"
)

func TestParseV1(t *testing.T) {
	data, err := os.ReadFile("testdata/basic.json")
	if err != nil {
		t.Fatalf("failed to read test file: %v", err)
	}

	ctx := context.Background()
	eszip, err := ParseBytes(ctx, data)
	if err != nil {
		t.Fatalf("failed to parse eszip: %v", err)
	}

	if !eszip.IsV1() {
		t.Fatal("expected V1 eszip")
	}

	specifier := "https://gist.githubusercontent.com/lucacasonato/f3e21405322259ca4ed155722390fda2/raw/e25acb49b681e8e1da5a2a33744b7a36d538712d/hello.js"
	module := eszip.GetModule(specifier)
	if module == nil {
		t.Fatalf("expected to find module: %s", specifier)
	}

	if module.Specifier != specifier {
		t.Errorf("expected specifier %s, got %s", specifier, module.Specifier)
	}

	if module.Kind != ModuleKindJavaScript {
		t.Errorf("expected JavaScript module, got %v", module.Kind)
	}

	source, err := module.Source(ctx)
	if err != nil {
		t.Fatalf("failed to get source: %v", err)
	}

	if len(source) == 0 {
		t.Error("expected non-empty source")
	}

	// Verify source contains expected content
	if !bytes.Contains(source, []byte("Hello World")) {
		t.Error("source should contain 'Hello World'")
	}
}

func TestParseV2(t *testing.T) {
	data, err := os.ReadFile("testdata/redirect.eszip2")
	if err != nil {
		t.Fatalf("failed to read test file: %v", err)
	}

	ctx := context.Background()
	eszip, err := ParseBytes(ctx, data)
	if err != nil {
		t.Fatalf("failed to parse eszip: %v", err)
	}

	if !eszip.IsV2() {
		t.Fatal("expected V2 eszip")
	}

	module := eszip.GetModule("file:///main.ts")
	if module == nil {
		t.Fatal("expected to find module: file:///main.ts")
	}

	if module.Kind != ModuleKindJavaScript {
		t.Errorf("expected JavaScript module, got %v", module.Kind)
	}

	source, err := module.Source(ctx)
	if err != nil {
		t.Fatalf("failed to get source: %v", err)
	}

	expectedSource := `export * as a from "./a.ts";
`
	if string(source) != expectedSource {
		t.Errorf("expected source %q, got %q", expectedSource, string(source))
	}

	// Test source map
	sourceMap, err := module.SourceMap(ctx)
	if err != nil {
		t.Fatalf("failed to get source map: %v", err)
	}

	if len(sourceMap) == 0 {
		t.Error("expected non-empty source map")
	}
}

func TestV2Redirect(t *testing.T) {
	data, err := os.ReadFile("testdata/redirect.eszip2")
	if err != nil {
		t.Fatalf("failed to read test file: %v", err)
	}

	ctx := context.Background()
	eszip, err := ParseBytes(ctx, data)
	if err != nil {
		t.Fatalf("failed to parse eszip: %v", err)
	}

	// file:///a.ts is a redirect to file:///b.ts
	moduleA := eszip.GetModule("file:///a.ts")
	if moduleA == nil {
		t.Fatal("expected to find module: file:///a.ts")
	}

	moduleB := eszip.GetModule("file:///b.ts")
	if moduleB == nil {
		t.Fatal("expected to find module: file:///b.ts")
	}

	sourceA, _ := moduleA.Source(ctx)
	sourceB, _ := moduleB.Source(ctx)

	// Both should have the same source since a.ts redirects to b.ts
	if !bytes.Equal(sourceA, sourceB) {
		t.Errorf("expected same source for redirect, got %q and %q", string(sourceA), string(sourceB))
	}
}

func TestTakeSource(t *testing.T) {
	data, err := os.ReadFile("testdata/basic.json")
	if err != nil {
		t.Fatalf("failed to read test file: %v", err)
	}

	ctx := context.Background()
	eszip, err := ParseBytes(ctx, data)
	if err != nil {
		t.Fatalf("failed to parse eszip: %v", err)
	}

	specifier := "https://gist.githubusercontent.com/lucacasonato/f3e21405322259ca4ed155722390fda2/raw/e25acb49b681e8e1da5a2a33744b7a36d538712d/hello.js"
	module := eszip.GetModule(specifier)
	if module == nil {
		t.Fatalf("expected to find module: %s", specifier)
	}

	// Take the source
	source, err := module.TakeSource(ctx)
	if err != nil {
		t.Fatalf("failed to take source: %v", err)
	}

	if len(source) == 0 {
		t.Error("expected non-empty source")
	}

	// Module should no longer be available in V1
	module2 := eszip.GetModule(specifier)
	if module2 != nil {
		t.Error("expected module to be removed after take (V1 behavior)")
	}
}

func TestV2TakeSource(t *testing.T) {
	data, err := os.ReadFile("testdata/redirect.eszip2")
	if err != nil {
		t.Fatalf("failed to read test file: %v", err)
	}

	ctx := context.Background()
	eszip, err := ParseBytes(ctx, data)
	if err != nil {
		t.Fatalf("failed to parse eszip: %v", err)
	}

	module := eszip.GetModule("file:///main.ts")
	if module == nil {
		t.Fatal("expected to find module")
	}

	// Take the source
	source, err := module.TakeSource(ctx)
	if err != nil {
		t.Fatalf("failed to take source: %v", err)
	}

	if len(source) == 0 {
		t.Error("expected non-empty source")
	}

	// Module should still be available but source should be nil
	module2 := eszip.GetModule("file:///main.ts")
	if module2 == nil {
		t.Fatal("expected module to still exist in V2")
	}

	source2, err := module2.Source(ctx)
	if err != nil {
		t.Fatalf("failed to get source: %v", err)
	}
	if source2 != nil {
		t.Error("expected source to be nil after take")
	}

	// Source map should still be available
	sourceMap, err := module2.SourceMap(ctx)
	if err != nil {
		t.Fatalf("failed to get source map: %v", err)
	}
	if len(sourceMap) == 0 {
		t.Error("expected source map to still be available")
	}
}

func TestV2Specifiers(t *testing.T) {
	data, err := os.ReadFile("testdata/redirect.eszip2")
	if err != nil {
		t.Fatalf("failed to read test file: %v", err)
	}

	ctx := context.Background()
	eszip, err := ParseBytes(ctx, data)
	if err != nil {
		t.Fatalf("failed to parse eszip: %v", err)
	}

	specs := eszip.Specifiers()
	if len(specs) == 0 {
		t.Error("expected at least one specifier")
	}

	// Should contain main.ts, b.ts, and a.ts
	expected := map[string]bool{
		"file:///main.ts": true,
		"file:///b.ts":    true,
		"file:///a.ts":    true,
	}

	for _, spec := range specs {
		delete(expected, spec)
	}

	if len(expected) > 0 {
		t.Errorf("missing specifiers: %v", expected)
	}
}

func TestNewV2AndWrite(t *testing.T) {
	ctx := context.Background()

	// Create a new V2 eszip
	eszip := NewV2()

	// Add a module
	eszip.AddModule("file:///test.js", ModuleKindJavaScript, []byte("console.log('hello');"), []byte("{}"))

	// Add a redirect
	eszip.AddRedirect("file:///alias.js", "file:///test.js")

	// Serialize
	data, err := eszip.IntoBytes()
	if err != nil {
		t.Fatalf("failed to serialize eszip: %v", err)
	}

	// Parse it back
	parsed, err := ParseBytes(ctx, data)
	if err != nil {
		t.Fatalf("failed to parse serialized eszip: %v", err)
	}

	if !parsed.IsV2() {
		t.Fatal("expected V2 eszip")
	}

	// Verify the module
	module := parsed.GetModule("file:///test.js")
	if module == nil {
		t.Fatal("expected to find module")
	}

	source, err := module.Source(ctx)
	if err != nil {
		t.Fatalf("failed to get source: %v", err)
	}

	if string(source) != "console.log('hello');" {
		t.Errorf("expected source %q, got %q", "console.log('hello');", string(source))
	}

	// Verify the redirect
	aliasModule := parsed.GetModule("file:///alias.js")
	if aliasModule == nil {
		t.Fatal("expected to find alias module")
	}

	aliasSource, err := aliasModule.Source(ctx)
	if err != nil {
		t.Fatalf("failed to get alias source: %v", err)
	}

	if string(aliasSource) != "console.log('hello');" {
		t.Errorf("expected alias source %q, got %q", "console.log('hello');", string(aliasSource))
	}
}

func TestChecksumTypes(t *testing.T) {
	testCases := []struct {
		name     string
		checksum ChecksumType
	}{
		{"NoChecksum", ChecksumNone},
		{"Sha256", ChecksumSha256},
		{"XxHash3", ChecksumXxh3},
	}

	ctx := context.Background()

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			eszip := NewV2()
			eszip.SetChecksum(tc.checksum)
			eszip.AddModule("file:///test.js", ModuleKindJavaScript, []byte("test"), nil)

			data, err := eszip.IntoBytes()
			if err != nil {
				t.Fatalf("failed to serialize: %v", err)
			}

			parsed, err := ParseBytes(ctx, data)
			if err != nil {
				t.Fatalf("failed to parse: %v", err)
			}

			module := parsed.GetModule("file:///test.js")
			if module == nil {
				t.Fatal("expected to find module")
			}

			source, err := module.Source(ctx)
			if err != nil {
				t.Fatalf("failed to get source: %v", err)
			}

			if string(source) != "test" {
				t.Errorf("expected source 'test', got %q", string(source))
			}
		})
	}
}

func TestModuleKinds(t *testing.T) {
	testCases := []struct {
		kind ModuleKind
		name string
	}{
		{ModuleKindJavaScript, "javascript"},
		{ModuleKindJson, "json"},
		{ModuleKindJsonc, "jsonc"},
		{ModuleKindOpaqueData, "opaque_data"},
		{ModuleKindWasm, "wasm"},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			if tc.kind.String() != tc.name {
				t.Errorf("expected %s, got %s", tc.name, tc.kind.String())
			}
		})
	}
}

func TestV1Iterator(t *testing.T) {
	data, err := os.ReadFile("testdata/basic.json")
	if err != nil {
		t.Fatalf("failed to read test file: %v", err)
	}

	eszip, err := ParseV1(data)
	if err != nil {
		t.Fatalf("failed to parse eszip: %v", err)
	}

	modules := eszip.Iterate()
	if len(modules) != 1 {
		t.Errorf("expected 1 module, got %d", len(modules))
	}
}

func TestV2Iterator(t *testing.T) {
	data, err := os.ReadFile("testdata/redirect.eszip2")
	if err != nil {
		t.Fatalf("failed to read test file: %v", err)
	}

	ctx := context.Background()
	parsed, err := ParseBytes(ctx, data)
	if err != nil {
		t.Fatalf("failed to parse eszip: %v", err)
	}

	modules := parsed.V2().Iterate()
	// Should have 3 modules but only 2 are actual modules (one is redirect)
	if len(modules) < 2 {
		t.Errorf("expected at least 2 modules, got %d", len(modules))
	}
}
