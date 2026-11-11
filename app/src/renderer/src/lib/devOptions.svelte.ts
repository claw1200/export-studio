import { storedFlag, storeFlag } from './deck/stored'

const KEY = 'devOptions'

class DevOptions {
  enabled = $state(storedFlag(KEY))

  setEnabled(on: boolean): void {
    this.enabled = on
    storeFlag(KEY, on)
  }
}

export const devOptions = new DevOptions()
