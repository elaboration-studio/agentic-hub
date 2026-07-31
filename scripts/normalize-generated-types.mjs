import { readdir, readFile, writeFile } from "node:fs/promises";

const generatedTypesDir = new URL("../src/types/generated/", import.meta.url);
const entries = await readdir(generatedTypesDir, { withFileTypes: true });

await Promise.all(
  entries
    .filter((entry) => entry.isFile() && entry.name.endsWith(".ts"))
    .map(async (entry) => {
      const file = new URL(entry.name, generatedTypesDir);
      const source = await readFile(file, "utf8");
      const normalized = `${source
        .split(/\r?\n/)
        .map((line) => line.trimEnd())
        .join("\n")
        .replace(/\n*$/, "")}\n`;
      if (normalized !== source) await writeFile(file, normalized, "utf8");
    }),
);
