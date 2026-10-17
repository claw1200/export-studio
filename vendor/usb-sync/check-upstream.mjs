import { execFileSync } from 'node:child_process'
import { readFileSync, readdirSync } from 'node:fs'
import { dirname, join, resolve, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = dirname(fileURLToPath(import.meta.url))
const provenance = JSON.parse(readFileSync(join(root, 'upstream.json'), 'utf8'))
const checkout = process.argv[2]
if (!checkout) throw new Error('Usage: node check-upstream.mjs <upstream-checkout>')
const upstream = resolve(checkout)
const revision = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: upstream, encoding: 'utf8' }).trim()
if (revision !== provenance.commit) throw new Error(`Expected upstream commit ${provenance.commit}, got ${revision}`)

function files(directory, base = directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name)
    return entry.isDirectory() ? files(path, base) : [relative(base, path)]
  }).sort()
}

for (const crate of [...provenance.productionCrates, ...provenance.testSupportCrates]) {
  const source = join(upstream, 'crates', crate)
  const destination = join(root, 'crates', crate)
  const expectedFiles = files(source)
  if (JSON.stringify(files(destination)) !== JSON.stringify(expectedFiles)) throw new Error(`File set differs: ${crate}`)
  for (const file of expectedFiles) {
    let expected = readFileSync(join(source, file))
    if (crate === 'rbl-export' && file.endsWith('.rs')) {
      expected = Buffer.from(expected.toString('utf8')
        .replaceAll('.rbxport-publication', '.exportstudio-publication')
        .replaceAll('rbxport/manifest.json', 'exportstudio/manifest.json')
        .replaceAll('rbxport/recovered-', 'exportstudio/recovered-')
        .replaceAll('"PIONEER/rbxport"', '"PIONEER/exportstudio"'))
    }
    if (!expected.equals(readFileSync(join(destination, file)))) throw new Error(`Unexpected downstream change: ${crate}/${file}`)
  }
}
console.log(`Verified ${provenance.version} (${revision}) with documented namespace changes`)