<script lang="ts">
  import { DatabaseBackup, FolderOpen, LoaderCircle } from '@lucide/svelte'
  import { INTERVAL_CHOICES, KEEP_CHOICES } from '$lib/backups.svelte'
  import { getApp } from '$lib/context'
  import { DEFAULT_LIBRARY_PREFS } from '$lib/library.svelte'
  import type { BackupProgress } from '$lib/types'
  import type { Choice } from './choice'
  import { allOf, setting } from './resettable'
  import SavedBackups from './SavedBackups.svelte'
  import SettingChoice from './SettingChoice.svelte'
  import SettingsGroup from './SettingsGroup.svelte'
  import SettingsPage from './SettingsPage.svelte'
  import SettingToggle from './SettingToggle.svelte'

  let { title }: { title: string } = $props()

  const { library, backups } = getApp()

  const KEEP: Choice[] = KEEP_CHOICES.map((n) => ({ id: String(n), label: String(n) }))
  const INTERVAL: Choice[] = INTERVAL_CHOICES.map((c) => ({ id: String(c.hours), label: c.label }))

  const defaults = allOf(
    setting(
      () => library.writeFileTags,
      DEFAULT_LIBRARY_PREFS.writeFileTags,
      (on) => library.setWriteFileTags(on)
    ),
    { isDefault: () => backups.settingsAreDefault, reset: () => void backups.resetSettings() }
  )

  function progressLabel(p: BackupProgress): string {
    const count = p.total ? ` ${p.done ?? 0} of ${p.total} files` : ''
    if (p.stage === 'checking') return p.reason === 'restore' ? 'Checking the backup…' : 'Checking the library…'
    if (p.stage === 'extracting') return `Restoring${count}`
    return `Saving${count}`
  }
</script>

<SettingsPage {title} {defaults}>
  <SettingsGroup title="Files">
    <SettingToggle
      label="Write track info to file tags"
      description="When you edit title, artist, album and other text fields, also save them into the audio files’ own tags. Off keeps changes in rekordbox only. Rating and colour always stay in rekordbox."
      on={library.writeFileTags}
      onflip={() => library.setWriteFileTags(!library.writeFileTags)}
    />
  </SettingsGroup>

  <SettingsGroup title="Backups">
    <SettingToggle
      label="Automatic backups"
      description="Back up the library before the first edit each time you unlock editing, and periodically while the app is open. Skipped when nothing has changed."
      on={backups.settings.auto}
      onflip={() => void backups.setSettings({ auto: !backups.settings.auto })}
    />
    <SettingChoice
      label="Minimum time between backups"
      description="Automatic backups are skipped if one was already taken within this time."
      options={INTERVAL}
      current={String(backups.settings.interval)}
      onpick={(id) => void backups.setSettings({ interval: Number(id) })}
    />
    <SettingChoice
      label="Keep"
      description="How many automatic backups to keep. Manual backups stay until you delete them."
      options={KEEP}
      current={String(backups.settings.keep)}
      onpick={(id) => void backups.setSettings({ keep: Number(id) })}
    />
    <p class="px-2 pt-1 text-[11px] leading-snug text-faint">
      Backups hold the rekordbox database, playlists, cues, beatgrids, waveforms and artwork. Audio files are not backed
      up.
    </p>

    <div class="mt-5 flex items-center gap-2 px-2">
      <button
        class="flex h-7 items-center gap-1.5 rounded-md bg-accent px-3 text-[12px] font-medium text-bg hover:bg-accent/90 disabled:pointer-events-none disabled:opacity-50"
        disabled={!!backups.busy}
        onclick={() => void backups.create()}
      >
        <DatabaseBackup class="size-3.5" />
        Back up now
      </button>
      <button
        class="flex h-7 items-center gap-1.5 rounded-md px-2 text-[12px] text-muted hover:bg-hover hover:text-fg disabled:pointer-events-none disabled:opacity-40"
        disabled={!backups.list.length}
        onclick={() => backups.list[0] && void window.api.showItemInFolder(backups.list[0].path)}
      >
        <FolderOpen class="size-3.5" />
        Show backups folder
      </button>
      {#if backups.progress}
        {@const p = backups.progress}
        <span class="ml-auto flex min-w-0 items-center gap-2 text-[11px] text-muted" role="status">
          <LoaderCircle class="size-3 shrink-0 animate-spin text-accent" />
          <span class="truncate">{progressLabel(p)}</span>
          {#if p.total}
            <span class="h-1 w-20 shrink-0 overflow-hidden rounded-full bg-line">
              <span
                class="block h-full rounded-full bg-accent transition-[width]"
                style:width="{(100 * (p.done ?? 0)) / p.total}%"
              ></span>
            </span>
          {/if}
        </span>
      {/if}
    </div>

    {#if backups.error}
      <p class="mx-2 mt-3 rounded-md border border-warn/60 bg-warn/10 px-2 py-1.5 text-[11.5px] text-warn">
        {backups.error}
      </p>
    {/if}
  </SettingsGroup>

  <SettingsGroup title="Saved backups">
    <SavedBackups />
  </SettingsGroup>
</SettingsPage>
