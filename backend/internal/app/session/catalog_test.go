package session

import (
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"
)

func fixtureDir(t *testing.T) string {
	t.Helper()
	base, err := filepath.Abs("../../../../tmp/session-catalog-tests")
	if err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(base, 0700); err != nil {
		t.Fatal(err)
	}
	dir, err := os.MkdirTemp(base, "case-")
	if err != nil {
		t.Fatal(err)
	}
	return dir
}
func write(t *testing.T, dir, name, text string) string {
	t.Helper()
	path := filepath.Join(dir, name)
	if err := os.WriteFile(path, []byte(text), 0600); err != nil {
		t.Fatal(err)
	}
	return path
}
func sessionHeader(id string) string {
	return `{"type":"session","version":3,"id":"` + id + `","timestamp":"2026-06-23T09:51:39.256Z","cwd":"/workspace","parentSession":"/synthetic/origin.jsonl"}` + "\n"
}
func message(role, content, timestamp string) string {
	return `{"type":"message","id":"entry","parentId":null,"timestamp":"` + timestamp + `","message":{"role":"` + role + `","model":"synthetic","content":` + content + `}}` + "\n"
}
func TestMetadataAndReadOnly(t *testing.T) {
	dir := fixtureDir(t)
	text := "\n" + sessionHeader("id") + message("assistant", `[{"type":"text","text":"answer"}]`, "2026-01") + message("user", `[{"type":"text","text":"你好"},{"type":"image","data":"synthetic","mimeType":"image/png"},{"type":"text","text":"world"}]`, "2026-03") + message("toolResult", `[]`, "9999") +
		`{"type":"message","id":"tool","timestamp":"2026-04","message":{"role":"toolResult","toolCallId":"call","toolName":"Read","content":[{"type":"text","text":"not indexed"}]}}` + "\n" +
		`{"type":"session_info","id":"info","timestamp":"2026-05","name":"  latest name  "}` + "\n" +
		`{"type":"session_info","id":"older","timestamp":"2026-02","name":"old"}` + "\n" +
		`{"type":"label","id":"bad","timestamp":"9999"}` + "\n" + "bad json\n"
	path := write(t, dir, "valid.jsonl", text)
	before, _ := os.Stat(path)
	metas, err := ListDir(dir)
	if err != nil || len(metas) != 1 {
		t.Fatalf("%v %v", metas, err)
	}
	metadata := metas[0]
	if metadata.ID != "id" || metadata.Path != path || metadata.Cwd != "/workspace" || metadata.Created != "2026-06-23T09:51:39.256Z" || metadata.ParentSessionPath == nil || *metadata.ParentSessionPath != "/synthetic/origin.jsonl" || metadata.Name == nil || *metadata.Name != "  latest name  " || metadata.Modified != "2026-05" || metadata.MessageCount != 3 || metadata.FirstMessage != "你好 world" || metadata.AllMessagesText != "answer 你好 world" {
		t.Fatalf("metadata mismatch: %+v", metadata)
	}
	after, _ := os.Stat(path)
	actual, _ := os.ReadFile(path)
	if string(actual) != text || !before.ModTime().Equal(after.ModTime()) {
		t.Fatal("catalog changed JSONL")
	}
}
func TestLayersSortEmptyAndErrors(t *testing.T) {
	global, project := fixtureDir(t), fixtureDir(t)
	write(t, global, "shared.jsonl", sessionHeader("shared")+message("user", `"global"`, "2026-09"))
	write(t, project, "shared.jsonl", sessionHeader("shared")+message("user", `"project"`, "2026-01"))
	write(t, global, "only.jsonl", sessionHeader("global-only")+message("user", `"only"`, "2026-02"))
	metas, err := ListDirs([]string{global, project})
	if err != nil || len(metas) != 2 || metas[0].ID != "global-only" || metas[1].FirstMessage != "project" || metas[1].Path != filepath.Join(project, "shared.jsonl") {
		t.Fatalf("%+v %v", metas, err)
	}
	for _, dirs := range [][]string{nil, {filepath.Join(global, "missing")}, {fixtureDir(t)}} {
		metas, err := ListDirs(dirs)
		if err != nil || len(metas) != 0 {
			t.Fatalf("expected empty: %v %v", metas, err)
		}
	}
	if _, err := (Catalog{}).GetSessions(); err == nil || err.Error() != "No session directories configured" {
		t.Fatalf("unexpected error: %v", err)
	}
	file := write(t, global, "not-directory", "x")
	if _, err := ListDirs([]string{project, file}); err == nil || err.Error() != "Failed to read session dir: "+file {
		t.Fatalf("directory error: %v", err)
	}
	entries, err := (Catalog{[]string{filepath.Join(global, "missing")}}).GetSessions()
	if err != nil {
		t.Fatal(err)
	}
	data, _ := json.Marshal(entries)
	if string(data) != "[]" {
		t.Fatalf("empty result: %s", data)
	}
}
func TestInvalidFilesSkippedAndMtime(t *testing.T) {
	dir := fixtureDir(t)
	for i, bad := range []string{"", "not-json", `{"type":"message"}`, `{"type":"session","version":3,"id":"x","timestamp":"t"}`, `{"type":"session","version":null,"id":"x","timestamp":"t","cwd":"c"}`} {
		write(t, dir, string(rune('a'+i))+".jsonl", bad)
	}
	write(t, dir, "ignored.JSONL", sessionHeader("ignored"))
	if err := os.Mkdir(filepath.Join(dir, "directory.jsonl"), 0700); err != nil {
		t.Fatal(err)
	}
	path := write(t, dir, "empty.jsonl", sessionHeader("empty"))
	stamp := time.Date(2026, 1, 2, 3, 4, 5, 123000000, time.UTC)
	if err := os.Chtimes(path, stamp, stamp); err != nil {
		t.Fatal(err)
	}
	metas, err := ListDir(dir)
	if err != nil || len(metas) != 1 || metas[0].Modified != "2026-01-02T03:04:05.123+00:00" || metas[0].FirstMessage != "(no messages)" || metas[0].AllMessagesText != "" || metas[0].MessageCount != 0 {
		t.Fatalf("%+v %v", metas, err)
	}
}
func TestAllEntryVariantsAndNames(t *testing.T) {
	dir := fixtureDir(t)
	lines := []string{
		`{"type":"thinking_level_change","thinkingLevel":"high"}`,
		`{"type":"thinking_effort_change","thinkingEffort":"low"}`,
		`{"type":"model_change","provider":"openai","modelId":"test"}`,
		`{"type":"compaction","summary":"sum","firstKeptEntryId":"e","tokensBefore":12,"fromHook":true}`,
		`{"type":"custom","customType":"x","data":null}`,
		`{"type":"label","targetId":"e","label":null}`,
	}
	for i, line := range lines {
		line = strings.TrimSuffix(line, "}") + `,"id":"e","timestamp":"activity","parentId":null}`
		entry, ok := parseEntry([]byte(line))
		if !ok || entry.timestamp != "activity" {
			t.Fatalf("variant %d invalid: %s", i, line)
		}
	}
	text := sessionHeader("named") + message("user", `"first"`, "1") +
		`{"type":"session_info","id":"i","timestamp":"3","name":"  "}` + "\n" +
		`{"type":"session_info","id":"i2","timestamp":"3","name":"equal timestamp ignored"}` + "\n" +
		`{"type":"session_info","id":"i3","timestamp":"2","name":"old"}` + "\n"
	path := write(t, dir, "name.jsonl", text)
	metadata, err := buildMeta(path)
	if err != nil || metadata.Name != nil || metadata.Modified != "3" {
		t.Fatalf("%+v %v", metadata, err)
	}
	for _, line := range []string{
		`{"type":"custom","id":"e","timestamp":"z","customType":null}`,
		`{"type":"session_info","id":"e","timestamp":"z","name":12}`,
		`{"type":"compaction","id":"e","timestamp":"z","summary":"s","firstKeptEntryId":"f","tokensBefore":-1}`,
		`{"type":"unknown","id":"e","timestamp":"z"}`,
		`{"type":"message","id":"e","timestamp":"z","message":{"role":"user","content":[{"type":"text"}]}}`,
		`{"type":"message","id":"e","timestamp":"z","message":{"role":"assistant","content":null,"model":"x"}}`,
	} {
		if _, ok := parseEntry([]byte(line)); ok {
			t.Fatalf("accepted malformed entry: %s", line)
		}
	}
}
func TestPreviewUnicodeAndLongLine(t *testing.T) {
	dir := fixtureDir(t)
	text := strings.Repeat("玫🌹", 26)
	content, _ := json.Marshal(text)
	write(t, dir, "unicode.jsonl", sessionHeader("u")+message("user", string(content), "2"))
	long, _ := json.Marshal(strings.Repeat("x", 100000))
	write(t, dir, "long.jsonl", sessionHeader("l")+message("user", string(long), "1"))
	entries, err := (Catalog{[]string{dir}}).GetSessions()
	if err != nil || len(entries) != 2 || entries[0].Name != strings.Repeat("玫🌹", 25)+"..." {
		t.Fatalf("%+v %v", entries, err)
	}
	metas, _ := ListDir(dir)
	if len(metas[1].FirstMessage) != 100000 {
		t.Fatal("long line truncated")
	}
	data, _ := json.Marshal(entries[0])
	var fields map[string]json.RawMessage
	_ = json.Unmarshal(data, &fields)
	keys := []string{}
	for key := range fields {
		keys = append(keys, key)
	}
	if len(keys) != 5 || fields["message_count"] == nil {
		t.Fatalf("wire fields: %s", data)
	}
	if !reflect.DeepEqual(entries[0], ListEntry{"u", filepath.Join(dir, "unicode.jsonl"), strings.Repeat("玫🌹", 25) + "...", "2", 1}) {
		t.Fatal("entry differs")
	}
}

func TestCorruptUnicodeAndDuplicateHeader(t *testing.T) {
	dir := fixtureDir(t)
	write(t, dir, "surrogate.jsonl", `{"type":"session","version":3,"id":"\ud800","timestamp":"t","cwd":"c"}`)
	write(t, dir, "duplicate.jsonl", `{"type":"session","version":3,"id":"a","id":"b","timestamp":"t","cwd":"c"}`)
	write(t, dir, "wrong-case.jsonl", `{"Type":"session","Version":3,"Id":"case","Timestamp":"t","Cwd":"c"}`)
	write(t, dir, "valid.jsonl", sessionHeader("valid")+message("user", `"ok \ud83c\udf39"`, "1")+message("user", `"broken \ud800"`, "9"))
	metas, err := ListDir(dir)
	if err != nil || len(metas) != 1 || metas[0].MessageCount != 1 || metas[0].Modified != "1" || metas[0].FirstMessage != "ok 🌹" {
		t.Fatalf("corrupt JSON influenced catalog: %+v %v", metas, err)
	}
}
