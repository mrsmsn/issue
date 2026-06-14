package main

import (
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
)

// Deterministic corpus generator for the issue-CLI benchmark.
// Produces N frontmatter-bearing markdown files into <outdir>.
// Stdlib only; reproducible via a fixed LCG seed.

var statuses = []string{"open", "closed", "in-progress", "wontfix"}
var types = []string{"feature", "bug", "epic", "chore", "spike", "docs"}
var labelPool = []string{"cli", "mvp", "perf", "ux", "infra", "parser", "storage", "tui", "export", "import", "good-first-issue", "P1", "P2", "P3"}
var words = []string{"add", "fix", "refactor", "implement", "investigate", "remove", "support", "improve", "handle", "parse", "render", "benchmark", "migrate", "document", "list", "create", "view", "lint", "id", "frontmatter", "merge", "conflict", "sort", "filter", "concurrent", "loader", "schema", "slug", "init", "export"}

func main() {
	n := 5000
	out := "/tmp/issue-bench/issue"
	if len(os.Args) > 1 {
		n, _ = strconv.Atoi(os.Args[1])
	}
	if len(os.Args) > 2 {
		out = os.Args[2]
	}
	if err := os.MkdirAll(out, 0o755); err != nil {
		panic(err)
	}

	var seed uint64 = 0x9e3779b97f4a7c15
	rnd := func(m int) int {
		seed = seed*6364136223846793005 + 1442695040888963407
		return int((seed >> 33) % uint64(m))
	}

	for id := 1; id <= n; id++ {
		title := titleFor(rnd)
		status := statuses[rnd(len(statuses))]
		typ := types[rnd(len(types))]
		nl := rnd(4) // 0..3 labels
		var labels []string
		seen := map[string]bool{}
		for i := 0; i < nl; i++ {
			l := labelPool[rnd(len(labelPool))]
			if !seen[l] {
				seen[l] = true
				labels = append(labels, l)
			}
		}
		nr := rnd(3) // 0..2 related
		var related []string
		for i := 0; i < nr && id > 1; i++ {
			related = append(related, strconv.Itoa(1+rnd(id-1)))
		}
		day := 1 + rnd(28)
		date := fmt.Sprintf("2026-%02d-%02d", 1+rnd(12), day)

		var b strings.Builder
		b.WriteString("---\n")
		b.WriteString("id: " + strconv.Itoa(id) + "\n")
		b.WriteString("title: \"" + title + "\"\n")
		b.WriteString("status: " + status + "\n")
		b.WriteString("type: " + typ + "\n")
		b.WriteString("created: " + date + "\n")
		b.WriteString("updated: " + date + "\n")
		b.WriteString("labels: [" + strings.Join(labels, ", ") + "]\n")
		b.WriteString("related: [" + strings.Join(related, ", ") + "]\n")
		b.WriteString("---\n\n")
		b.WriteString("## Background\n\n")
		for p := 0; p < 3; p++ {
			for w := 0; w < 25; w++ {
				b.WriteString(words[rnd(len(words))])
				b.WriteByte(' ')
			}
			b.WriteString("\n\n")
		}

		fname := strconv.Itoa(id) + "-" + slug(title) + ".md"
		if err := os.WriteFile(filepath.Join(out, fname), []byte(b.String()), 0o644); err != nil {
			panic(err)
		}
	}
	fmt.Printf("generated %d issues into %s\n", n, out)
}

func titleFor(rnd func(int) int) string {
	nw := 3 + rnd(5)
	parts := make([]string, nw)
	for i := range parts {
		parts[i] = words[rnd(len(words))]
	}
	t := strings.Join(parts, " ")
	return strings.ToUpper(t[:1]) + t[1:]
}

func slug(s string) string {
	var b strings.Builder
	prevDash := false
	for _, r := range strings.ToLower(s) {
		if (r >= 'a' && r <= 'z') || (r >= '0' && r <= '9') {
			b.WriteRune(r)
			prevDash = false
		} else if !prevDash {
			b.WriteByte('-')
			prevDash = true
		}
	}
	return strings.Trim(b.String(), "-")
}
