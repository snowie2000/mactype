import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

function listProductionRustSources(directory) {
  const sources = [];
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      if (entry.name !== "tests") sources.push(...listProductionRustSources(entryPath));
    } else if (entry.isFile() && entry.name.endsWith(".rs") && entry.name !== "tests.rs") {
      sources.push(entryPath);
    }
  }
  return sources.sort((left, right) => (left < right ? -1 : left > right ? 1 : 0));
}

const schema = JSON.parse(fs.readFileSync(path.join(root, "shared/settings-schema.json"), "utf8"));
const pairs = new Set(schema.map((setting) => `${setting.section}/${setting.key}`));
const required = [
  ["General", "UseMapping"], ["General", "UseInclude"], ["General", "FontSubstitutes"],
  ["General", "CacheMaxFaces"], ["General", "CacheMaxSizes"], ["General", "CacheMaxBytes"],
  ["DirectWrite", "GammaValue"], ["DirectWrite", "Contrast"], ["DirectWrite", "RenderingMode"], ["DirectWrite", "ClearTypeLevel"],
  ["Experimental", "ClipBoxFix"], ["Experimental", "ColorFont"], ["Experimental", "InvertColor"],
];
const missing = required.filter(([section, key]) => !pairs.has(`${section}/${key}`));
if (missing.length) throw new Error(`Settings schema is missing core settings: ${missing.map((pair) => pair.join("/")).join(", ")}`);
if (pairs.has("General/HookChildProcesses")) throw new Error("Retired HookChildProcesses setting must not be exposed by the editor");
if (schema.some((setting) => setting.section === "Infinality")) throw new Error("Unsupported Infinality settings must not be exposed by the editor");
if (schema.length !== 37) throw new Error(`Expected 37 supported scalar settings, found ${schema.length}`);

const defaultProfile = fs.readFileSync(path.join(root, "distribution/ini/Default.ini"), "utf8");
if (/^HookChildProcesses\s*=/m.test(defaultProfile)) throw new Error("Default profile must not enable retired HookChildProcesses setting");

const rustRoot = path.join(root, "control-center/src-tauri/src");
const profileSources = [
  path.join(rustRoot, "profile.rs"),
  ...listProductionRustSources(path.join(rustRoot, "profile")),
];
const profile = profileSources.map((source) => fs.readFileSync(source, "utf8")).join("\n");
for (const key of ["Shadow", "LcdFilterWeight", "PixelLayout", "FontSubstitutes", "UnloadDLL", "ExcludeSub"]) {
  if (!profile.includes(`\"${key}\"`)) throw new Error(`Structured profile editor is missing ${key}`);
}

const settingsHeader = fs.readFileSync(path.join(root, "settings.h"), "utf8");
const shadowOffset = settingsHeader.match(/case ATTR_ShadowOffset:([\s\S]*?)case ATTR_Fontlink:/)?.[1] ?? "";
if (!/\bbreak\s*;/.test(shadowOffset)) throw new Error("ATTR_ShadowOffset still falls through into ATTR_Fontlink");
for (const attribute of ["ATTR_FontSubstitute", "ATTR_DirectWrite", "ATTR_PixelLayout"]) {
  if ((settingsHeader.match(new RegExp(`case ${attribute}:`, "g")) ?? []).length < 2) throw new Error(`${attribute} must support both SetIntAttribute and GetIntAttribute`);
}
console.log("Settings coverage gate passed for 37 supported scalar settings, structured INI settings, and IControlCenter fallthrough guards.");
