// Copyright 2018-2024 the Deno authors. All rights reserved. MIT license.

// eszip is a CLI tool for working with eszip archives.
package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	eszip "github.com/example/eszip-go"
)

func main() {
	if len(os.Args) < 2 {
		printUsage()
		os.Exit(1)
	}

	command := os.Args[1]

	switch command {
	case "view", "v":
		viewCmd(os.Args[2:])
	case "extract", "x":
		extractCmd(os.Args[2:])
	case "create", "c":
		createCmd(os.Args[2:])
	case "info", "i":
		infoCmd(os.Args[2:])
	case "help", "-h", "--help":
		printUsage()
	default:
		fmt.Fprintf(os.Stderr, "Unknown command: %s\n\n", command)
		printUsage()
		os.Exit(1)
	}
}

func printUsage() {
	fmt.Println(`eszip - A tool for working with eszip archives

Usage:
  eszip <command> [options]

Commands:
  view, v       View contents of an eszip archive
  extract, x    Extract files from an eszip archive
  create, c     Create a new eszip archive from files
  info, i       Show information about an eszip archive
  help          Show this help message

Examples:
  eszip view archive.eszip2
  eszip view -s file:///main.ts archive.eszip2
  eszip extract -o ./output archive.eszip2
  eszip create -o archive.eszip2 file1.js file2.js
  eszip info archive.eszip2

Run 'eszip <command> -h' for more information on a command.`)
}

// viewCmd handles the 'view' command
func viewCmd(args []string) {
	fs := flag.NewFlagSet("view", flag.ExitOnError)
	specifier := fs.String("s", "", "Show only this specifier")
	showSourceMap := fs.Bool("m", false, "Show source maps")
	fs.Usage = func() {
		fmt.Println(`Usage: eszip view [options] <archive>

View the contents of an eszip archive.

Options:`)
		fs.PrintDefaults()
	}

	fs.Parse(args)
	if fs.NArg() < 1 {
		fs.Usage()
		os.Exit(1)
	}

	archivePath := fs.Arg(0)
	ctx := context.Background()

	archive, err := loadArchive(ctx, archivePath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error: %v\n", err)
		os.Exit(1)
	}

	specifiers := archive.Specifiers()
	for _, spec := range specifiers {
		if *specifier != "" && spec != *specifier {
			continue
		}

		module := archive.GetModule(spec)
		if module == nil {
			// Might be a redirect-only or npm specifier
			continue
		}

		fmt.Printf("Specifier: %s\n", spec)
		fmt.Printf("Kind: %s\n", module.Kind)
		fmt.Println("---")

		source, err := module.Source(ctx)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error getting source: %v\n", err)
			continue
		}

		if source != nil {
			fmt.Println(string(source))
		} else {
			fmt.Println("(source taken)")
		}

		if *showSourceMap {
			sourceMap, err := module.SourceMap(ctx)
			if err == nil && len(sourceMap) > 0 {
				fmt.Println("--- Source Map ---")
				fmt.Println(string(sourceMap))
			}
		}

		fmt.Println("============")
	}
}

// extractCmd handles the 'extract' command
func extractCmd(args []string) {
	fs := flag.NewFlagSet("extract", flag.ExitOnError)
	outputDir := fs.String("o", ".", "Output directory")
	fs.Usage = func() {
		fmt.Println(`Usage: eszip extract [options] <archive>

Extract files from an eszip archive.

Options:`)
		fs.PrintDefaults()
	}

	fs.Parse(args)
	if fs.NArg() < 1 {
		fs.Usage()
		os.Exit(1)
	}

	archivePath := fs.Arg(0)
	ctx := context.Background()

	archive, err := loadArchive(ctx, archivePath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error: %v\n", err)
		os.Exit(1)
	}

	specifiers := archive.Specifiers()
	for _, spec := range specifiers {
		module := archive.GetModule(spec)
		if module == nil {
			continue
		}

		// Skip data: URLs
		if strings.HasPrefix(spec, "data:") {
			continue
		}

		source, err := module.Source(ctx)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error getting source for %s: %v\n", spec, err)
			continue
		}

		if source == nil {
			continue
		}

		// Convert specifier to file path
		filePath := specifierToPath(spec)
		fullPath := filepath.Join(*outputDir, filePath)

		// Create parent directories
		if err := os.MkdirAll(filepath.Dir(fullPath), 0755); err != nil {
			fmt.Fprintf(os.Stderr, "Error creating directory: %v\n", err)
			continue
		}

		// Write file
		if err := os.WriteFile(fullPath, source, 0644); err != nil {
			fmt.Fprintf(os.Stderr, "Error writing file: %v\n", err)
			continue
		}

		fmt.Printf("Extracted: %s\n", fullPath)

		// Also extract source map if available
		sourceMap, err := module.SourceMap(ctx)
		if err == nil && len(sourceMap) > 0 {
			mapPath := fullPath + ".map"
			if err := os.WriteFile(mapPath, sourceMap, 0644); err == nil {
				fmt.Printf("Extracted: %s\n", mapPath)
			}
		}
	}
}

// createCmd handles the 'create' command
func createCmd(args []string) {
	fs := flag.NewFlagSet("create", flag.ExitOnError)
	outputPath := fs.String("o", "output.eszip2", "Output file path")
	checksum := fs.String("checksum", "sha256", "Checksum algorithm (none, sha256, xxhash3)")
	fs.Usage = func() {
		fmt.Println(`Usage: eszip create [options] <files...>

Create a new eszip archive from files.

Options:`)
		fs.PrintDefaults()
		fmt.Println(`
Examples:
  eszip create -o app.eszip2 main.js utils.js
  eszip create -checksum none -o app.eszip2 *.js`)
	}

	fs.Parse(args)
	if fs.NArg() < 1 {
		fs.Usage()
		os.Exit(1)
	}

	archive := eszip.NewV2()

	// Set checksum
	switch *checksum {
	case "none":
		archive.SetChecksum(eszip.ChecksumNone)
	case "sha256":
		archive.SetChecksum(eszip.ChecksumSha256)
	case "xxhash3":
		archive.SetChecksum(eszip.ChecksumXxh3)
	default:
		fmt.Fprintf(os.Stderr, "Unknown checksum: %s\n", *checksum)
		os.Exit(1)
	}

	// Add files
	for _, filePath := range fs.Args() {
		absPath, err := filepath.Abs(filePath)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error resolving path %s: %v\n", filePath, err)
			os.Exit(1)
		}

		content, err := os.ReadFile(absPath)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error reading file %s: %v\n", filePath, err)
			os.Exit(1)
		}

		// Determine module kind
		kind := eszip.ModuleKindJavaScript
		ext := strings.ToLower(filepath.Ext(filePath))
		switch ext {
		case ".json":
			kind = eszip.ModuleKindJson
		case ".wasm":
			kind = eszip.ModuleKindWasm
		}

		specifier := "file://" + absPath
		archive.AddModule(specifier, kind, content, nil)
		fmt.Printf("Added: %s\n", specifier)
	}

	// Serialize
	data, err := archive.IntoBytes()
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error serializing archive: %v\n", err)
		os.Exit(1)
	}

	// Write output
	if err := os.WriteFile(*outputPath, data, 0644); err != nil {
		fmt.Fprintf(os.Stderr, "Error writing output: %v\n", err)
		os.Exit(1)
	}

	fmt.Printf("Created: %s (%d bytes)\n", *outputPath, len(data))
}

// infoCmd handles the 'info' command
func infoCmd(args []string) {
	fs := flag.NewFlagSet("info", flag.ExitOnError)
	fs.Usage = func() {
		fmt.Println(`Usage: eszip info <archive>

Show information about an eszip archive.`)
	}

	fs.Parse(args)
	if fs.NArg() < 1 {
		fs.Usage()
		os.Exit(1)
	}

	archivePath := fs.Arg(0)
	ctx := context.Background()

	// Get file size
	stat, err := os.Stat(archivePath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error: %v\n", err)
		os.Exit(1)
	}

	archive, err := loadArchive(ctx, archivePath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error: %v\n", err)
		os.Exit(1)
	}

	specifiers := archive.Specifiers()

	fmt.Printf("File: %s\n", archivePath)
	fmt.Printf("Size: %d bytes\n", stat.Size())

	if archive.IsV1() {
		fmt.Println("Format: V1 (JSON)")
	} else {
		fmt.Println("Format: V2 (binary)")
	}

	fmt.Printf("Modules: %d\n", len(specifiers))

	// Count by kind
	kindCounts := make(map[eszip.ModuleKind]int)
	redirectCount := 0
	totalSourceSize := 0

	for _, spec := range specifiers {
		module := archive.GetModule(spec)
		if module == nil {
			redirectCount++
			continue
		}
		kindCounts[module.Kind]++

		source, _ := module.Source(ctx)
		totalSourceSize += len(source)
	}

	fmt.Println("\nModule types:")
	for kind, count := range kindCounts {
		fmt.Printf("  %s: %d\n", kind, count)
	}
	if redirectCount > 0 {
		fmt.Printf("  redirects: %d\n", redirectCount)
	}

	fmt.Printf("\nTotal source size: %d bytes\n", totalSourceSize)

	// Check for npm snapshot
	if archive.IsV2() {
		snapshot := archive.V2().TakeNpmSnapshot()
		if snapshot != nil {
			fmt.Printf("\nNPM packages: %d\n", len(snapshot.Packages))
			fmt.Printf("NPM root packages: %d\n", len(snapshot.RootPackages))
		}
	}
}

func loadArchive(ctx context.Context, path string) (*eszip.EszipUnion, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("failed to read file: %w", err)
	}

	return eszip.ParseBytes(ctx, data)
}

func specifierToPath(specifier string) string {
	// Remove protocol prefixes
	path := specifier
	for _, prefix := range []string{"file:///", "file://", "https://", "http://"} {
		if strings.HasPrefix(path, prefix) {
			path = strings.TrimPrefix(path, prefix)
			break
		}
	}

	// Clean the path
	path = strings.TrimPrefix(path, "/")

	return path
}
