package arch_test

// Installed by the-office as an architecture fitness function.
//
// This is a computational sensor for a rule that is otherwise only in
// reviewers' heads: which packages may import which. Edit `rules` to match the
// repo's real layering, then this test fails on drift instead of a human
// noticing it in review three weeks later.

import (
	"go/build"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// forbidden[pkgPrefix] = prefixes that pkgPrefix must not import.
var forbidden = map[string][]string{
	// "internal/domain": {"internal/http", "internal/db"}, // domain stays pure
}

func TestArchitecture(t *testing.T) {
	if len(forbidden) == 0 {
		t.Skip("no boundary rules declared yet — edit forbidden in arch_test.go")
	}
	root, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	err = filepath.Walk(root, func(path string, info os.FileInfo, err error) error {
		if err != nil || !info.IsDir() {
			return err
		}
		name := info.Name()
		if name == "vendor" || name == ".git" || strings.HasPrefix(name, ".") && name != "." {
			return filepath.SkipDir
		}
		rel, _ := filepath.Rel(root, path)
		rel = filepath.ToSlash(rel)

		pkg, err := build.ImportDir(path, 0)
		if err != nil {
			return nil // not a Go package
		}
		for prefix, banned := range forbidden {
			if !strings.HasPrefix(rel, prefix) {
				continue
			}
			for _, imp := range pkg.Imports {
				for _, b := range banned {
					if strings.Contains(imp, b) {
						t.Errorf("boundary violation: %s imports %s (forbidden by rule %q)", rel, imp, prefix)
					}
				}
			}
		}
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
}
