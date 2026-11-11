<script lang="ts">
  import { ExternalLink, LoaderCircle, RotateCcw, Star } from '@lucide/svelte'
  import { REPO_URL, type Update } from '../../../../../shared/updates'
  import appIcon from '../../../../../../resources/icon.png'
  import SettingsPage from './SettingsPage.svelte'

  let { title }: { title: string } = $props()

  let update = $state<Update | null | 'checking'>(null)
  let checkedForUpdate = $state(false)

  async function checkForUpdate(): Promise<void> {
    update = 'checking'
    update = await window.api.checkForUpdate()
    checkedForUpdate = true
  }
</script>

<SettingsPage {title}>
  <div class="flex flex-col items-center px-2 pt-8 text-center">
    <img src={appIcon} alt="" class="size-20" />
    <h3 class="mt-3 text-[16px] font-semibold">Export Studio</h3>
    <p class="mt-0.5 text-[12px] text-muted">A modern library browser for rekordbox</p>
    <p class="mt-3 font-mono text-[11.5px] text-faint tabular-nums">
      Version {window.api.version} · Electron {window.api.electronVersion}
    </p>

    <div class="mt-5 flex h-7 items-center gap-2 text-[12px]">
      {#if update === 'checking'}
        <span class="flex items-center gap-1.5 text-muted" role="status">
          <LoaderCircle class="size-3.5 animate-spin text-accent" />
          Checking for updates…
        </span>
      {:else if update}
        <a
          class="flex h-7 items-center gap-1.5 rounded-md bg-accent px-3 font-medium text-bg hover:bg-accent/90"
          href={update.url}
          target="_blank"
          rel="noreferrer"
        >
          Download version {update.version}
          <ExternalLink class="size-3.5" />
        </a>
      {:else}
        {#if checkedForUpdate}
          <span class="text-muted">No newer version found.</span>
        {/if}
        <button
          class="flex h-7 items-center gap-1.5 rounded-md px-2 text-muted hover:bg-hover hover:text-fg"
          onclick={() => void checkForUpdate()}
        >
          <RotateCcw class="size-3.5" />
          {checkedForUpdate ? 'Check again' : 'Check for updates'}
        </button>
      {/if}
    </div>

    <div class="mt-6 flex w-full max-w-sm items-center gap-3 rounded-lg border border-line-soft bg-bg/50 p-3 text-left">
      <Star class="size-5 shrink-0 text-accent" />
      <p class="min-w-0 flex-1 text-[11.5px] leading-snug text-muted">
        Find Export Studio useful? A star on GitHub helps other DJs find it.
      </p>
      <a
        class="flex h-7 shrink-0 items-center gap-1.5 rounded-md bg-accent px-3 text-[12px] font-medium text-bg hover:bg-accent/90"
        href={REPO_URL}
        target="_blank"
        rel="noreferrer"
      >
        <Star class="size-3.5" />
        Star
      </a>
    </div>

    <div class="mt-4 flex gap-1 text-[12px]">
      {@render externalLink('Release notes', `${REPO_URL}/releases`)}
      {@render externalLink('Report a problem', `${REPO_URL}/issues`)}
    </div>
  </div>
</SettingsPage>

{#snippet externalLink(label: string, href: string)}
  <a
    class="flex h-7 items-center gap-1.5 rounded-md px-2 text-muted hover:bg-hover hover:text-fg"
    {href}
    target="_blank"
    rel="noreferrer"
  >
    {label}
    <ExternalLink class="size-3" />
  </a>
{/snippet}
