import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, resolve, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(fileURLToPath(import.meta.url));
const provenance = JSON.parse(
  readFileSync(join(root, "upstream-sources.json"), "utf8"),
);
const checkout = process.argv[2];
if (!checkout)
  throw new Error("Usage: node check-upstream.mjs <upstream-checkout>");
const upstream = resolve(checkout);
const revision = execFileSync("git", ["rev-parse", "HEAD"], {
  cwd: upstream,
  encoding: "utf8",
}).trim();
if (revision !== provenance.commit)
  throw new Error(
    `Expected upstream commit ${provenance.commit}, got ${revision}`,
  );

function files(directory, base = directory) {
  return readdirSync(directory, { withFileTypes: true })
    .flatMap((entry) => {
      const path = join(directory, entry.name);
      return entry.isDirectory()
        ? files(path, base)
        : [relative(base, path).split(sep).join("/")];
    })
    .sort();
}

for (const crate of [
  ...provenance.productionCrates,
  ...provenance.testSupportCrates,
]) {
  const source = join(upstream, "crates", crate);
  const destination = join(root, "crates", crate);
  const expectedFiles = files(source);
  const relocations = provenance.fileRelocations ?? {};
  const localFiles = expectedFiles.filter(
    (file) => !relocations[`crates/${crate}/${file}`],
  );
  const extraFiles = (provenance.extraFiles?.[crate] ?? []).map((entry) => entry.path);
  if (JSON.stringify(files(destination)) !== JSON.stringify([...localFiles, ...extraFiles].sort()))
    throw new Error(`File set differs: ${crate}`);
  for (const file of expectedFiles) {
    let expected = readFileSync(join(source, file));
    const originalPath = `crates/${crate}/${file}`;
    const localPath = relocations[originalPath] ?? originalPath;
    const actual = readFileSync(join(root, localPath));
    const derived = provenance.derivedFiles?.[originalPath];
    if (derived) {
      const digest = (bytes) =>
        createHash("sha256").update(bytes).digest("hex");
      if (
        digest(expected) !== derived.upstreamSha256 ||
        digest(actual) !== derived.localSha256
      ) {
        throw new Error(`Unrecorded derived change: ${originalPath}`);
      }
      continue;
    }
    if (file === "Cargo.toml") {
      expected = Buffer.from(
        expected
          .toString("utf8")
          .replace(
            "version.workspace = true",
            `version = "${provenance.version}"`,
          ),
      );
    }
    if (crate === "rbl-export" && file.endsWith(".rs")) {
      expected = Buffer.from(
        expected
          .toString("utf8")
          .replaceAll(".rbxport-publication", ".exportstudio-publication")
          .replaceAll("rbxport/manifest.json", "exportstudio/manifest.json")
          .replaceAll("rbxport/recovered-", "exportstudio/recovered-")
          .replaceAll('"PIONEER/rbxport"', '"PIONEER/exportstudio"'),
      );
    }
    if (!expected.equals(actual))
      throw new Error(`Unexpected downstream change: ${crate}/${file}`);
  }
  for (const entry of provenance.extraFiles?.[crate] ?? []) {
    const digest = createHash("sha256")
      .update(readFileSync(join(root, `crates/${crate}/${entry.path}`)))
      .digest("hex");
    if (digest !== entry.sha256) throw new Error(`Unexpected extra file: ${crate}/${entry.path}`);
  }
}
for (const name of ["LICENSE", "COPYING"]) {
  const original =
    name === "COPYING"
      ? join(upstream, "crates", "rbl-deck", "vendor", "rubberband", name)
      : join(upstream, name);
  if (
    !readFileSync(original).equals(
      readFileSync(join(root, "licenses", "rbxport", name)),
    )
  ) {
    throw new Error(`License differs: ${name}`);
  }
}
console.log(
  `Verified ${provenance.version} (${revision}) with documented namespace and workspace changes`,
);
