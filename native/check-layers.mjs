import { execFileSync } from "node:child_process";
import { dirname, isAbsolute, join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const native = dirname(fileURLToPath(import.meta.url));
const graph = JSON.parse(
  execFileSync(
    "cargo",
    ["metadata", "--locked", "--no-deps", "--format-version", "1"],
    {
      cwd: native,
      encoding: "utf8",
      maxBuffer: 16 * 1024 * 1024,
    },
  ),
);
const sdk = graph.packages.filter(
  (entry) => entry.name.startsWith("rb-") || entry.name.startsWith("rbl-"),
);
const packages = new Map(graph.packages.map((entry) => [entry.name, entry]));
function runtimeDependencies(name, visited = new Set()) {
  if (visited.has(name)) return visited;
  visited.add(name);
  for (const dependency of packages.get(name)?.dependencies ?? []) {
    if (dependency.kind !== "dev")
      runtimeDependencies(dependency.name, visited);
  }
  return visited;
}
for (const name of ["rb-library", "rbl-db", "rbl-index"]) {
  if (runtimeDependencies("rb-usb").has(name))
    throw new Error(
      `USB service depends on master-library implementation ${name}`,
    );
}
if (runtimeDependencies("rb-library").has("rb-usb"))
  throw new Error("Master-library access depends on USB orchestration");
for (const entry of sdk) {
  for (const dependency of entry.dependencies.filter(
    (dependency) => dependency.kind !== "dev",
  )) {
    if (dependency.name.startsWith("es-"))
      throw new Error(
        `${entry.name} depends on application crate ${dependency.name}`,
      );
    if (entry.name === "rb-model" && dependency.name !== "serde")
      throw new Error(`Domain model depends on ${dependency.name}`);
    if (dependency.path) {
      const location = relative(
        join(graph.workspace_root, "crates"),
        dependency.path,
      );
      if (
        isAbsolute(location) ||
        location === ".." ||
        location.startsWith(".." + sep)
      ) {
        throw new Error(
          `${entry.name} depends on a path outside the SDK workspace`,
        );
      }
    }
  }
}
for (const name of [
  "rb-model",
  "rb-storage",
  "rb-anlz",
  "rb-library",
  "rb-device",
  "rb-usb",
]) {
  if (!sdk.some((entry) => entry.name === name))
    throw new Error(`Missing SDK layer ${name}`);
}
console.log(
  `Verified ${sdk.length} SDK crates: no runtime application dependencies; model is pure; USB and master-library access are independent`,
);
