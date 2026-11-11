import { describe, expect, it } from 'vitest'
import { startSteps } from './GetStarted'

describe('startSteps', () => {
  it('coche chaque étape d’après les données du projet', () => {
    const none = startSteps(1, 0, null, undefined).map((s) => s.done)
    expect(none).toEqual([false, false, false])
    const all = startSteps(1, 2, { interval_minutes: 60, enabled: true, next_run_at: 0 } as never, { integrations: { gsc: { property: 'sc-domain:x' } } } as never)
    expect(all.map((s) => s.done)).toEqual([true, true, true])
    // Une planification désactivée ne compte pas.
    expect(startSteps(1, 1, { enabled: false } as never, {} as never)[1].done).toBe(false)
  })
})
