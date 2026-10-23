import { useState } from 'react'
import { Link } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { QueryError } from '@/components/QueryError'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Label } from '@/components/ui/label'
import { api } from '@/lib/api'
import { formatDate } from '@/lib/format'
import { INTERVALS, intervalLabel, untilLabel } from '@/lib/schedule'
import { isDesktop } from '@/lib/transport'

/** Audit récurrent du projet : un intervalle, la prochaine échéance, le dernier audit lancé. */
export function ScheduleCard({ projectId }: { projectId: number }) {
  const queryClient = useQueryClient()
  const key = ['schedule', projectId]
  const schedule = useQuery({ queryKey: key, queryFn: () => api.schedule(projectId), refetchInterval: 30_000 })
  const [minutes, setMinutes] = useState(60 * 24)
  const save = useMutation({
    mutationFn: (body: { interval_minutes: number; enabled: boolean }) => api.saveSchedule(projectId, body),
    onSuccess: (s) => queryClient.setQueryData(key, s),
  })
  const remove = useMutation({
    mutationFn: () => api.deleteSchedule(projectId),
    onSuccess: () => queryClient.setQueryData(key, null),
  })
  const s = schedule.data
  const options = s && !INTERVALS.some((i) => i.minutes === s.interval_minutes) ? [{ minutes: s.interval_minutes, label: intervalLabel(s.interval_minutes) }, ...INTERVALS] : INTERVALS

  return (
    <Card>
      <CardHeader>
        <CardTitle>Planification</CardTitle>
        <CardDescription>
          Relance l’audit automatiquement, avec la configuration du projet.{' '}
          {isDesktop
            ? 'Dans l’application, seulement tant qu’elle est ouverte.'
            : 'Tant que le serveur tourne ; un audit manqué pendant un arrêt est lancé au redémarrage.'}{' '}
          Un audit en cours bloque le suivant. Intervalle minimal : 1 heure.
        </CardDescription>
      </CardHeader>
      <CardContent className="grid gap-3">
        <form
          className="flex flex-wrap items-end gap-3"
          onSubmit={(e) => {
            e.preventDefault()
            save.mutate({ interval_minutes: s?.interval_minutes ?? minutes, enabled: s?.enabled ?? true })
          }}
        >
          <div className="grid gap-1.5">
            <Label htmlFor="sched-interval">Fréquence</Label>
            <select
              id="sched-interval"
              className="field"
              value={s?.interval_minutes ?? minutes}
              onChange={(e) => {
                const m = Number(e.target.value)
                setMinutes(m)
                if (s) save.mutate({ interval_minutes: m, enabled: s.enabled })
              }}
            >
              {options.map((i) => (
                <option key={i.minutes} value={i.minutes}>
                  {i.label}
                </option>
              ))}
            </select>
          </div>
          {!s && <Button type="submit" disabled={save.isPending}>Planifier</Button>}
          {s && (
            <>
              <Button type="button" variant="outline" disabled={save.isPending} onClick={() => save.mutate({ interval_minutes: s.interval_minutes, enabled: !s.enabled })}>
                {s.enabled ? 'Mettre en pause' : 'Réactiver'}
              </Button>
              <Button type="button" variant="ghost" disabled={remove.isPending} onClick={() => remove.mutate()}>
                Supprimer
              </Button>
            </>
          )}
        </form>
        {s && (
          <p className="text-sm text-muted-foreground" role="status">
            {s.enabled ? (
              <>
                Prochain audit : {formatDate(s.next_run_at)} ({untilLabel(s.next_run_at)})
              </>
            ) : (
              'Planification en pause.'
            )}
            {s.last_run_id !== null && (
              <>
                {' '}
                · dernier audit planifié :{' '}
                <Link to={`/crawls/${s.last_run_id}`} className="underline underline-offset-4">
                  #{s.last_run_id}
                </Link>
              </>
            )}
          </p>
        )}
        <QueryError error={schedule.error ?? save.error ?? remove.error} />
      </CardContent>
    </Card>
  )
}
