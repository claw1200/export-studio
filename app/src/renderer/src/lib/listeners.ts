export class Listeners<Args extends unknown[]> {
  private readonly listeners = new Set<(...args: Args) => void>()

  add(listener: (...args: Args) => void): () => void {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  notify(...args: Args): void {
    for (const listener of this.listeners) listener(...args)
  }
}
