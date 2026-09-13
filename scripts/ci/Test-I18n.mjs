import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const readJson = (relative) => JSON.parse(fs.readFileSync(path.join(root, relative), "utf8"));
const locales = ["ko", "en", "zh-CN", "zh-TW", "ja", "fr", "de", "es", "pt", "ar"];
const catalogs = Object.fromEntries(locales.map((locale) => [locale, readJson(`control-center/src/i18n/${locale}.json`)]));
const ko = catalogs.ko;
const en = catalogs.en;
const schema = readJson("shared/settings-schema.json");
const settingIds = new Set(schema.map((setting) => setting.id));

const koKeys = Object.keys(ko).sort();
const placeholders = (message) => [...message.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort();
for (const [locale, catalog] of Object.entries(catalogs)) {
  const keys = Object.keys(catalog).sort();
  if (JSON.stringify(koKeys) !== JSON.stringify(keys)) {
    const missing = koKeys.filter((key) => !(key in catalog));
    const unexpected = keys.filter((key) => !(key in ko));
    throw new Error(`Catalog keys differ for ${locale}. Missing: ${missing.join(", ") || "none"}; unexpected: ${unexpected.join(", ") || "none"}`);
  }
  for (const key of koKeys) {
    if (!catalog[key].trim()) throw new Error(`Empty translation in ${locale}: ${key}`);
    const settingKey = key.match(/^settings\.([^.]+)\./);
    if (settingKey && !settingIds.has(settingKey[1])) {
      throw new Error(`Orphaned setting translation in ${locale}: ${key}`);
    }
    if (JSON.stringify(placeholders(ko[key])) !== JSON.stringify(placeholders(catalog[key]))) {
      throw new Error(`Placeholder mismatch in ${locale}: ${key}`);
    }
  }
}

for (const setting of schema) {
  for (const field of ["label", "description"]) {
    const key = `settings.${setting.id}.${field}`;
    for (const [locale, catalog] of Object.entries(catalogs)) {
      if (!(key in catalog)) throw new Error(`Missing ${locale} setting translation: ${key}`);
    }
  }
  for (const option of setting.options ?? []) {
    const key = `settings.${setting.id}.option.${option.value}`;
    for (const [locale, catalog] of Object.entries(catalogs)) {
      if (!(key in catalog)) throw new Error(`Missing ${locale} setting option translation: ${key}`);
    }
  }
}

const isLanguageAutonym = (key) => key.startsWith("app.language") && key !== "app.language";
for (const [key, message] of Object.entries(en)) {
  if (/[가-힣一-龯ぁ-んァ-ンء-ي]/u.test(message) && !isLanguageAutonym(key)) {
    throw new Error(`Unexpected non-Latin script in English catalog: ${key}`);
  }
}

const scriptCoverage = [
  ["ko", /[가-힣]/u],
  ["zh-CN", /[一-龯]/u],
  ["zh-TW", /[一-龯]/u],
  ["ja", /[ぁ-んァ-ン]/u],
  ["ar", /[ء-ي]/u],
];
for (const [locale, pattern] of scriptCoverage) {
  const translated = Object.entries(catalogs[locale]).filter(([key, message]) => !isLanguageAutonym(key) && pattern.test(message)).length;
  if (translated < 150) throw new Error(`Insufficient ${locale} script coverage: ${translated}/${koKeys.length}`);
}

for (const locale of ["fr", "de", "es", "pt"]) {
  const identical = koKeys.filter((key) => !isLanguageAutonym(key) && catalogs[locale][key] === en[key]);
  if (identical.length > 30) throw new Error(`Too many untranslated ${locale} messages: ${identical.length}`);
}

const koFontGlyphs = new Set(fs.readFileSync(path.join(root, "control-center/src/assets/fonts/ko-glyphs.txt"), "utf8"));
const missingGlyphs = new Set();
for (const message of Object.values(ko)) {
  for (const character of message) {
    if (character.codePointAt(0) > 0x7e && !/\s/u.test(character) && !koFontGlyphs.has(character)) missingGlyphs.add(character);
  }
}
if (missingGlyphs.size > 0) {
  throw new Error(`Korean UI font subset is stale; regenerate with scripts/generate-ko-font-subset.py. Missing glyphs: ${[...missingGlyphs].join("")}`);
}

console.log(`i18n catalog gate passed for ${locales.length} locales, ${koKeys.length} messages, and ${schema.length} settings.`);
