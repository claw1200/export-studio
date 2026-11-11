<script lang="ts">
  import { getApp } from '$lib/context'
  import {
    CURVES,
    DEFAULT_AUTO_MASTER,
    DEFAULT_CURVE_ID,
    DEFAULT_KEY_HIGHLIGHT_ID,
    DEFAULT_SWAP_TIMING_ID,
    KEY_HIGHLIGHTS,
    SWAP_TIMINGS
  } from '$lib/mix/prefs.svelte'
  import MixCurvePreview from '../mix/MixCurvePreview.svelte'
  import { allOf, setting } from './resettable'
  import SettingChoice from './SettingChoice.svelte'
  import SettingsPage from './SettingsPage.svelte'
  import SettingToggle from './SettingToggle.svelte'

  let { title }: { title: string } = $props()

  const { decks } = getApp()
  const mixPrefs = decks.mixPrefs

  const defaults = allOf(
    setting(
      () => mixPrefs.curveId,
      DEFAULT_CURVE_ID,
      (id) => mixPrefs.setCurve(id)
    ),
    setting(
      () => mixPrefs.swapTimingId,
      DEFAULT_SWAP_TIMING_ID,
      (id) => mixPrefs.setSwapTiming(id)
    ),
    setting(
      () => mixPrefs.autoMaster,
      DEFAULT_AUTO_MASTER,
      (on) => mixPrefs.setAutoMaster(on)
    ),
    setting(
      () => mixPrefs.keyHighlightId,
      DEFAULT_KEY_HIGHLIGHT_ID,
      (id) => mixPrefs.setKeyHighlight(id)
    )
  )
</script>

<SettingsPage {title} {defaults}>
  <MixCurvePreview curve={mixPrefs.curve} range={mixPrefs.swapRange} />
  <div class="flex flex-col gap-1">
    <SettingChoice
      label="Crossfader curve"
      description="How quickly a deck fades out as the fader moves away from it."
      options={CURVES}
      current={mixPrefs.curveId}
      onpick={(id) => mixPrefs.setCurve(id)}
    />
    <SettingChoice
      label="Bass swap"
      description="When Swap bass is on, how soon the incoming deck takes the bass during a fade."
      options={SWAP_TIMINGS}
      current={mixPrefs.swapTimingId}
      onpick={(id) => mixPrefs.setSwapTiming(id)}
    />
    <SettingToggle
      label="Auto master"
      description="The deck that alone has sound, playing and not faded out, becomes the master."
      on={mixPrefs.autoMaster}
      onflip={() => mixPrefs.setAutoMaster(!mixPrefs.autoMaster)}
    />
    <SettingChoice
      label="Highlight matching keys"
      description="Colours browser keys that mix with the master deck. Pick the loosest match to show."
      options={KEY_HIGHLIGHTS}
      current={mixPrefs.keyHighlightId}
      onpick={(id) => mixPrefs.setKeyHighlight(id)}
    />
  </div>
</SettingsPage>
