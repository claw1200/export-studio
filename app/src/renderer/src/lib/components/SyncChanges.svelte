<script lang="ts">
  import { Check } from '@lucide/svelte'
  import { groupChanges, needsChoice, onlyResolution } from '$lib/syncReview'
  import type { SyncDiagnostic, SyncInspection, SyncResolution } from '../../../../preload/engine'

  let {
    deviceName,
    inspection,
    resolutions = {},
    locked,
    onresolve,
    onrecover
  }: {
    deviceName: string
    inspection: SyncInspection
    resolutions?: Record<string, SyncResolution>
    locked: boolean
    onresolve: (diagnostic: string, resolution: SyncResolution) => void
    onrecover: () => void
  } = $props()

  const RESOLUTION_LABELS: Record<SyncResolution, string> = {
    use_library: 'Use library version',
    restore_from_library: 'Restore from library',
    discard_device_edits: 'Discard device edits'
  }

  const OUTCOMES: Record<SyncResolution, string> = {
    use_library: 'The library’s version is used when you sync',
    restore_from_library: 'Restored from your library when you sync',
    discard_device_edits: 'Edits made on the USB are discarded when you sync'
  }

  const SEVERITY_BORDERS: Record<SyncDiagnostic['severity'], string> = {
    blocked: 'border-danger',
    decision_required: 'border-warn',
    information: 'border-line'
  }

  let showInformation = $state(false)
  const changes = $derived(groupChanges(inspection.diagnostics))
  const informationCount = $derived(changes.filter(({ diagnostic }) => diagnostic.severity === 'information').length)
  const visible = $derived(changes.filter(({ diagnostic }) => diagnostic.severity !== 'information' || showInformation))

  function resolveAll(ids: string[], resolution: SyncResolution): void {
    for (const id of ids) onresolve(id, resolution)
  }
</script>

<div class="flex flex-col gap-2.5" role="region" aria-label={`Changes on ${deviceName}`}>
  {#each visible as { ids, diagnostic } (ids[0])}
    {@const only = onlyResolution(diagnostic)}
    <div class={['break-words border-l-2 pl-2.5', only ? 'border-line' : SEVERITY_BORDERS[diagnostic.severity]]}>
      <p class="text-[12px] font-medium">{diagnostic.name || diagnostic.entity}</p>
      <p class="mt-0.5 text-[11px] leading-relaxed text-muted">{diagnostic.detail}</p>
      {#if only}
        <p class="mt-1 flex items-center gap-1 text-[11px] text-faint">
          <Check class="size-3 shrink-0" aria-hidden="true" />
          {OUTCOMES[only]}
        </p>
      {/if}
      {#if diagnostic.kind === 'recovery_required'}
        <button
          type="button"
          class="mt-1.5 h-7 rounded-md border border-line px-2 text-[11px] text-muted hover:bg-hover hover:text-fg disabled:opacity-40"
          disabled={locked}
          onclick={onrecover}
        >
          Recover interrupted export
        </button>
      {/if}
      {#if needsChoice(diagnostic)}
        <select
          class="mt-1.5 h-7 max-w-full rounded-md border border-line bg-bg px-2 text-[11px]"
          aria-label={`Resolution for ${diagnostic.name}`}
          value={resolutions[ids[0]] ?? ''}
          disabled={locked}
          onchange={(event) => {
            const value = event.currentTarget.value as SyncResolution | ''
            if (value) resolveAll(ids, value)
          }}
        >
          <option value="">Choose action</option>
          {#each diagnostic.allowedResolutions as resolution (resolution)}
            <option value={resolution}>{RESOLUTION_LABELS[resolution]}</option>
          {/each}
        </select>
      {/if}
    </div>
  {/each}
  {#if informationCount > 0}
    <button
      type="button"
      class="self-start text-[11px] text-muted hover:text-fg"
      onclick={() => (showInformation = !showInformation)}
    >
      {showInformation ? 'Hide' : 'Show'}
      {informationCount} informational {informationCount === 1 ? 'change' : 'changes'}
    </button>
  {/if}
</div>
