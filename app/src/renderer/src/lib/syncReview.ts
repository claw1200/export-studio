import type { SyncDiagnostic, SyncInspection, SyncResolution } from '../../../preload/engine'

/** One change as a person sees it: the stick's two databases each report it, under their own ids. */
export interface SyncChange {
  ids: string[]
  diagnostic: SyncDiagnostic
}

export function onlyResolution({ severity, allowedResolutions }: SyncDiagnostic): SyncResolution | null {
  return severity === 'decision_required' && allowedResolutions.length === 1 ? allowedResolutions[0] : null
}

export function needsChoice({ severity, allowedResolutions }: SyncDiagnostic): boolean {
  return severity === 'decision_required' && allowedResolutions.length > 1
}

export function automaticResolutions(inspection: SyncInspection): Record<string, SyncResolution> {
  const chosen: Record<string, SyncResolution> = {}
  for (const diagnostic of inspection.diagnostics) {
    const only = onlyResolution(diagnostic)
    if (only) chosen[diagnostic.id] = only
  }
  return chosen
}

export function groupChanges(diagnostics: SyncDiagnostic[]): SyncChange[] {
  const changes = new Map<string, SyncChange>()
  for (const diagnostic of diagnostics) {
    const { kind, severity, name, detail, allowedResolutions } = diagnostic
    const key = JSON.stringify([kind, severity, name, detail, allowedResolutions])
    const change = changes.get(key)
    if (change) change.ids.push(diagnostic.id)
    else changes.set(key, { ids: [diagnostic.id], diagnostic })
  }
  return [...changes.values()]
}
