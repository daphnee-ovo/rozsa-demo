package input

import (
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"testing"

	"rozsa/backend/internal/app/skills"
)

func TestCompleteReturnsBuiltinAndSkillCommands(t *testing.T) {
	service := NewService(nil)
	result, err := service.Complete(Request{
		Text:   "/revie",
		Cursor: 6,
		CWD:    t.TempDir(),
		Skills: []skills.Skill{{Name: "review", Description: "Review the current changes"}},
	})
	if err != nil {
		t.Fatal(err)
	}
	if result.Prefix != "/revie" {
		t.Fatalf("prefix = %q", result.Prefix)
	}
	if len(result.Items) != 1 || result.Items[0].Value != "/skill:review " {
		t.Fatalf("items = %#v", result.Items)
	}

	conflict, err := service.Complete(Request{
		Text:   "/skill:mod",
		Cursor: 10,
		CWD:    t.TempDir(),
		Skills: []skills.Skill{{Name: "model", Description: "Custom model workflow"}},
	})
	if err != nil {
		t.Fatal(err)
	}
	if len(conflict.Items) != 1 || conflict.Items[0].Value != "/skill:model " {
		t.Fatalf("builtin conflict should use the skill namespace: %#v", conflict.Items)
	}
}

func TestCompleteUsesUTF16CursorAndHighlightRanges(t *testing.T) {
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, "notes.txt"), []byte("notes"), 0o600); err != nil {
		t.Fatal(err)
	}
	text := "😀 /model @notes.txt"
	result, err := NewService(nil).Complete(Request{Text: text, Cursor: 9, CWD: root})
	if err != nil {
		t.Fatal(err)
	}
	if result.Prefix != "/model" || !result.ValidMatch {
		t.Fatalf("completion = %#v", result)
	}
	want := []HighlightRange{{Start: 3, End: 9}, {Start: 10, End: 20}}
	if !reflect.DeepEqual(result.HighlightRanges, want) {
		t.Fatalf("highlight ranges = %#v, want %#v", result.HighlightRanges, want)
	}
}

func TestCompleteHighlightsEveryValidSlashTokenAcrossEmojiText(t *testing.T) {
	text := "😀 /model and /skill:deploy"
	result, err := NewService(nil).Complete(Request{
		Text: text, Cursor: 27, CWD: t.TempDir(),
		Skills: []skills.Skill{{Name: "deploy", Description: "Deploy the current project"}},
	})
	if err != nil {
		t.Fatal(err)
	}
	want := []HighlightRange{{Start: 3, End: 9}, {Start: 14, End: 27}}
	if !reflect.DeepEqual(result.HighlightRanges, want) {
		t.Fatalf("highlight ranges = %#v, want %#v", result.HighlightRanges, want)
	}
}

func TestIsSlashCompletionUsesUtf16CursorAndCurrentToken(t *testing.T) {
	if !IsSlashCompletion("😀 /rev trailing", 6) {
		t.Fatal("slash token before the UTF-16 cursor was not detected")
	}
	if IsSlashCompletion("/model then plain", 17) {
		t.Fatal("non-slash current token was treated as a slash completion")
	}
}

func TestHasSkillTokenFindsCompletedSkillCommandsOutsideCurrentToken(t *testing.T) {
	for _, text := range []string{
		"/skill:deploy ",
		"review this /skill:deploy and summarize",
		"😀 /SKILL:deploy tail",
	} {
		if !HasSkillToken(text) {
			t.Errorf("HasSkillToken(%q) = false, want true", text)
		}
	}
	for _, text := range []string{"plain text", "/skill:", "https://example.test/skill:deploy"} {
		if HasSkillToken(text) {
			t.Errorf("HasSkillToken(%q) = true, want false", text)
		}
	}
}

func TestCompleteHighlightsSkillCommandWhenCaretIsAfterTrailingSpace(t *testing.T) {
	text := "/skill:deploy "
	result, err := NewService(nil).Complete(Request{
		Text: text, Cursor: utf16Length(text), CWD: t.TempDir(),
		Skills: []skills.Skill{{Name: "deploy", Description: "Deploy the current project"}},
	})
	if err != nil {
		t.Fatal(err)
	}
	want := []HighlightRange{{Start: 0, End: len("/skill:deploy")}}
	if !reflect.DeepEqual(result.HighlightRanges, want) {
		t.Fatalf("highlight ranges = %#v, want %#v", result.HighlightRanges, want)
	}
}

func TestCompleteFileReferencesSortsDirectoriesAndQuotesSpaces(t *testing.T) {
	root := t.TempDir()
	if err := os.Mkdir(filepath.Join(root, "folder"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "folder with space.txt"), nil, 0o600); err != nil {
		t.Fatal(err)
	}
	result, err := NewService(nil).Complete(Request{Text: "@fol", Cursor: 4, CWD: root})
	if err != nil {
		t.Fatal(err)
	}
	if result.Prefix != "@fol" || len(result.Items) != 2 {
		t.Fatalf("completion = %#v", result)
	}
	if result.Items[0].Value != "@folder/" || result.Items[1].Value != `@"folder with space.txt" ` {
		t.Fatalf("items = %#v", result.Items)
	}
}

func TestCompleteReturnsEmptyResultsForUnknownTokens(t *testing.T) {
	root := t.TempDir()
	service := NewService(nil)
	for _, request := range []Request{
		{Text: "/unknown-token", Cursor: len("/unknown-token"), CWD: root},
		{Text: "@missing", Cursor: len("@missing"), CWD: root},
	} {
		result, err := service.Complete(request)
		if err != nil {
			t.Fatal(err)
		}
		if len(result.Items) != 0 {
			t.Fatalf("unexpected suggestions for %q: %#v", request.Text, result.Items)
		}
	}
}

func TestCompleteRequiresWorkingDirectoryForFileReferences(t *testing.T) {
	_, err := NewService(nil).Complete(Request{Text: "@source", Cursor: len("@source")})
	if err == nil {
		t.Fatal("expected a missing working directory error")
	}
}

func TestCompleteSurfacesFileEnumerationErrors(t *testing.T) {
	service := NewService(failingFileSystem{})
	_, err := service.Complete(Request{Text: "@read", Cursor: 5, CWD: t.TempDir()})
	if err == nil {
		t.Fatal("expected directory read error")
	}
}

type failingFileSystem struct{}

func (failingFileSystem) ReadDir(string) ([]os.DirEntry, error) {
	return nil, errors.New("read denied")
}
func (failingFileSystem) Stat(name string) (os.FileInfo, error) { return os.Stat(name) }
