import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const projectRoot = resolve(import.meta.dirname, "..");
const sourceDirectory = resolve(projectRoot, "backend/resources/builtin-skills");
const configPath = resolve(projectRoot, "backend/config.yaml");
const names = readdirSync(sourceDirectory, { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .map((entry) => entry.name)
  .sort();

if (names.some((name) => name === "." || name === ".." || /[\\/:]/u.test(name))) {
  throw new Error("Built-in skill directory names cannot contain path separators");
}

const currentConfig = readFileSync(configPath, "utf8");
const skillListLine = `BUILTIN_SKILLS: ${JSON.stringify(names)}`;
const existingEntries = currentConfig.match(/^BUILTIN_SKILLS:.*$/gmu) ?? [];
if (existingEntries.length > 1) {
  throw new Error(`Expected one BUILTIN_SKILLS entry in ${configPath}`);
}

const updatedConfig = existingEntries.length === 1
  ? currentConfig.replace(/^BUILTIN_SKILLS:.*$/mu, skillListLine)
  : `${currentConfig.trimEnd()}\n${skillListLine}\n`;

if (updatedConfig !== currentConfig) {
  writeFileSync(configPath, updatedConfig);
}
