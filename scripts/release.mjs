// Collects the built installer into release/ with a checksum and install notes.
// Run via `npm run release` (builds first) or `node scripts/release.mjs` after a build.
import { createHash } from "node:crypto";
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const root = new URL("..", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
const conf = JSON.parse(readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8"));
const version = conf.version;
const targetDir = process.env.CARGO_TARGET_DIR || join(root, "src-tauri", "target");
const nsisDir = join(targetDir, "release", "bundle", "nsis");
const installer = `${conf.productName}_${version}_x64-setup.exe`;
const source = join(nsisDir, installer);
if (!existsSync(source)) {
  console.error(`Installer not found: ${source}\nBuild it first with \`npm run app:build\`.`);
  process.exit(1);
}

const outDir = join(root, "release", `v${version}`);
rmSync(outDir, { recursive: true, force: true });
mkdirSync(outDir, { recursive: true });
const published = installer.replace(/ /g, "-");
copyFileSync(source, join(outDir, published));
const sha = createHash("sha256").update(readFileSync(source)).digest("hex");
writeFileSync(join(outDir, "SHA256SUMS.txt"), `${sha}  ${published}\n`);
copyFileSync(join(root, "docs", "INSTALL.txt"), join(outDir, "How to install.txt"));
copyFileSync(join(root, "LICENSE.txt"), join(outDir, "LICENSE.txt"));

console.log(`Release ${version} ready in ${outDir}`);
for (const f of readdirSync(outDir)) console.log(`  ${f}`);
console.log(`SHA-256: ${sha}`);
