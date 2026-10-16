import {
  AudioWaveform,
  FileAudio,
  FilePlus,
  FileSearch,
  Folder,
  FolderSearch,
  ListMinus,
  ListMusic,
  ListPlus,
  LockOpen,
  Palette,
  Pencil,
  Plus,
  ScanSearch,
  Star,
  Tag,
  Trash2
} from '@lucide/svelte'
import { TRACK_COLORS } from './format'
import { matchesPreset, presetLabel } from './convertPresets'
import { loadPresets } from './convertPresetStore'
import type { Library } from './library.svelte'
import type { TreeNode } from './library/tree'
import { tidy, type MenuEntry } from './menu'
import type { Player } from './player.svelte'
import type { ConvertPreset, Track, TrackEdit } from './types'

export type TrackMenuShortcut =
  'library.editInfo' | 'library.removeFromPlaylist' | 'library.removeFromCollection' | 'library.analyse'

export interface TrackMenuActions {
  edit: (tracks: Track[]) => void
  autoRelocate: (tracks: Track[]) => void
  remove: (tracks: Track[]) => void
  analyse: (tracks: Track[]) => void
  convert: (tracks: Track[], preset: ConvertPreset) => void
  shortcut: (id: TrackMenuShortcut) => string | undefined
}

export const SEPARATOR: MenuEntry = { kind: 'separator' }

export function unlockEntry(library: Library, player: Player): MenuEntry {
  const blocked = !!library.info?.rekordboxRunning
  return {
    label: blocked ? 'Close rekordbox to edit' : 'Unlock editing',
    icon: LockOpen,
    disabled: blocked || library.switching,
    onselect: () =>
      void library.setWritable(true).then((changed) => {
        if (changed) void player.refreshDetail()
      })
  }
}
const REVEAL_LABEL = window.api.platform === 'darwin' ? 'Show in Finder' : 'Show in Explorer'

function playlistItems(nodes: TreeNode[], add: (id: string) => void): MenuEntry[] {
  return nodes.flatMap((node): MenuEntry[] => {
    const label = node.name || 'Untitled'
    if (node.kind === 'folder') {
      const children = playlistItems(node.children, add)
      return [{ label, icon: Folder, items: children.length ? children : [{ kind: 'header', label: 'No playlists' }] }]
    }
    if (node.kind !== 'playlist') return []
    return [{ label, icon: ListMusic, onselect: () => add(node.id) }]
  })
}

function tagItems(library: Library, tracks: Track[]): MenuEntry[] {
  const ids = tracks.map((t) => t.id)
  const item = (node: TreeNode): MenuEntry => {
    const tagged = tracks.filter((t) => t.myTags.includes(node.id)).length
    const checked = tagged === 0 ? false : tagged === tracks.length ? true : 'mixed'
    return {
      label: node.name || 'Untitled',
      checked,
      onselect: () => void library.setTag(node.id, ids, checked !== true)
    }
  }
  const out: MenuEntry[] = []
  for (const node of library.tagTree) {
    if (node.kind !== 'folder') {
      out.push(item(node))
      continue
    }
    const leaves = node.children.filter((child) => child.kind !== 'folder')
    if (leaves.length) out.push(SEPARATOR, { kind: 'header', label: node.name || 'Untitled' }, ...leaves.map(item))
  }
  return tidy(out)
}

function shared<K extends keyof Track>(tracks: Track[], key: K): Track[K] | undefined {
  const first = tracks[0]?.[key]
  return tracks.every((t) => t[key] === first) ? first : undefined
}

export function trackMenu(library: Library, player: Player, tracks: Track[], actions: TrackMenuActions): MenuEntry[] {
  const ids = tracks.map((t) => t.id)
  const single = tracks.length === 1 ? tracks[0] : null
  const locked = !library.writable
  const playlist = library.sourcePlaylist
  const update = (fields: TrackEdit): void => void library.updateTracks(ids, fields)
  const color = shared(tracks, 'color')
  const rating = shared(tracks, 'rating')
  const missing = tracks.filter((t) => t.missing && !t.id.startsWith('usb:'))
  const device = tracks.some((t) => t.id.startsWith('usb:'))
  const analysable = tracks.filter((t) => !t.missing && !t.streaming)
  const presets = loadPresets()
  return tidy([
    tracks.length > 1 && { kind: 'header', label: `${tracks.length} tracks` },
    !device && {
      label: single ? 'Edit info…' : `Edit info for ${tracks.length} tracks…`,
      icon: Pencil,
      shortcut: actions.shortcut('library.editInfo'),
      disabled: locked,
      onselect: () => actions.edit(tracks)
    },
    !device && SEPARATOR,
    !device && {
      label: 'Add to playlist',
      icon: ListPlus,
      disabled: locked,
      items: tidy([
        { label: 'New playlist', icon: Plus, onselect: () => void library.createPlaylist('root', false, ids) },
        SEPARATOR,
        ...playlistItems(library.playlistTree, (id) => void library.addToPlaylist(id, ids))
      ])
    },
    !device && playlist?.kind === 'playlist' && {
      label: 'Remove from playlist',
      icon: ListMinus,
      danger: true,
      shortcut: actions.shortcut('library.removeFromPlaylist'),
      disabled: locked,
      onselect: () => void library.removeFromPlaylist(playlist.id, ids)
    },
    !device && library.tagTree.length > 0 && { label: 'My Tag', icon: Tag, disabled: locked, items: tagItems(library, tracks) },
    !device && {
      label: 'Colour',
      icon: Palette,
      disabled: locked,
      items: [
        { label: 'No colour', checked: color === 0, onselect: () => update({ color: 0 }) },
        SEPARATOR,
        ...Object.entries(TRACK_COLORS).map(([n, c]) => ({
          label: c.name,
          swatch: c.hex,
          checked: color === Number(n),
          onselect: () => update({ color: Number(n) })
        }))
      ]
    },
    !device && {
      label: 'Rating',
      icon: Star,
      disabled: locked,
      items: [0, 1, 2, 3, 4, 5].map((n) => ({
        label: n ? '★'.repeat(n) + '☆'.repeat(5 - n) : 'No rating',
        checked: rating === n,
        onselect: () => update({ rating: n })
      }))
    },
    SEPARATOR,
    single &&
      missing.length > 0 && {
        label: 'Relocate…',
        icon: FileSearch,
        disabled: locked,
        onselect: () => void library.locate(single)
      },
    missing.length > 0 && {
      label: missing.length === 1 ? 'Auto-relocate…' : `Auto-relocate ${missing.length} tracks…`,
      icon: ScanSearch,
      disabled: locked,
      onselect: () => actions.autoRelocate(missing)
    },
    single &&
      !single.missing && {
        label: REVEAL_LABEL,
        icon: FolderSearch,
        onselect: () => window.api.showItemInFolder(single.path)
      },
    !device && SEPARATOR,
    !device && {
      label:
        (analysable.length && analysable.every((t) => t.analysed) ? 'Re-analyse' : 'Analyse') +
        (analysable.length > 1 ? ` ${analysable.length} tracks` : ''),
      icon: AudioWaveform,
      shortcut: actions.shortcut('library.analyse'),
      disabled: locked || !analysable.length,
      onselect: () => actions.analyse(analysable)
    },
    !device && {
      label: 'Convert file type',
      icon: FileAudio,
      disabled: locked || !analysable.length,
      items: presets.length
        ? presets.map((preset) => ({
            label: presetLabel(preset),
            disabled: analysable.length === 1 && matchesPreset(analysable[0], preset),
            onselect: () => actions.convert(analysable, preset)
          }))
        : [{ kind: 'header', label: 'Add formats in Settings' }]
    },
    !device && {
      label: single ? 'Remove from collection…' : `Remove ${tracks.length} tracks from collection…`,
      icon: Trash2,
      danger: true,
      shortcut: actions.shortcut('library.removeFromCollection'),
      disabled: locked,
      onselect: () => actions.remove(tracks)
    },
    locked && !device && SEPARATOR,
    locked && !device && unlockEntry(library, player)
  ])
}

/** Right-click menu for file-list rows that are not in the collection. */
export function fileMenu(
  library: Library,
  player: Player,
  tracks: Track[],
  actions: {
    importFiles: (paths: string[], playlistId: string | null) => void
    importAndAnalyse: (paths: string[]) => void
    deleteFiles: (tracks: Track[]) => void
  },
  includeUnlock = true
): MenuEntry[] {
  const paths = tracks.map((track) => track.path).filter(Boolean)
  const single = tracks.length === 1 ? tracks[0] : null
  const locked = !library.writable
  const count = tracks.length
  return tidy([
    count > 1 && { kind: 'header', label: `${count} files` },
    {
      label: count === 1 ? 'Import' : `Import ${count} files`,
      icon: FilePlus,
      disabled: locked || !paths.length,
      onselect: () => actions.importFiles(paths, null)
    },
    {
      label: count === 1 ? 'Import and analyse' : `Import and analyse ${count} files`,
      icon: AudioWaveform,
      disabled: locked || !paths.length,
      onselect: () => actions.importAndAnalyse(paths)
    },
    {
      label: 'Add to playlist',
      icon: ListPlus,
      disabled: locked || !paths.length,
      items: tidy([
        {
          label: 'New playlist',
          icon: Plus,
          onselect: () => {
            void library.importFiles(paths).then((result) => {
              if (result?.ids.length) void library.createPlaylist('root', false, result.ids)
            })
          }
        },
        SEPARATOR,
        ...playlistItems(library.playlistTree, (id) => actions.importFiles(paths, id))
      ])
    },
    single && {
      label: REVEAL_LABEL,
      icon: FolderSearch,
      onselect: () => window.api.showItemInFolder(single.path)
    },
    SEPARATOR,
    {
      label: count === 1 ? 'Delete file…' : `Delete ${count} files…`,
      icon: Trash2,
      danger: true,
      disabled: !paths.length,
      onselect: () => actions.deleteFiles(tracks)
    },
    includeUnlock && locked && SEPARATOR,
    includeUnlock && locked && unlockEntry(library, player)
  ])
}
