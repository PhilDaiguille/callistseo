import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { QueryError } from '@/components/QueryError'
import { Button } from '@/components/ui/button'
import { api } from '@/lib/api'

/** Suppression en deux temps : on dit ce qui part avec le projet, puis on demande confirmation. */
export function DeleteProject({ id, name, audits, running }: { id: number; name: string; audits: number; running: boolean }) {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [confirming, setConfirming] = useState(false)
  const remove = useMutation({
    mutationFn: () => api.deleteProject(id),
    onSuccess: () => {
      queryClient.removeQueries({ queryKey: ['project', id] })
      void queryClient.invalidateQueries({ queryKey: ['projects'] })
      navigate('/')
    },
  })

  if (!confirming) {
    return (
      <Button
        variant="outline"
        size="sm"
        disabled={running}
        title={running ? 'Un audit est en cours : arrêtez-le ou annulez-le d’abord.' : undefined}
        onClick={() => setConfirming(true)}
      >
        Supprimer le projet
      </Button>
    )
  }
  return (
    <div role="alertdialog" aria-label="Confirmer la suppression" className="panel-pop grid max-w-md gap-2 p-3">
      <p className="text-sm">
        Supprimer <b>{name}</b> ? {audits > 0 ? `Ses ${audits} audit${audits > 1 ? 's' : ''}, ` : 'Sa '}
        {audits > 0 ? 'sa planification et ses segments' : 'planification et ses segments'} seront effacés définitivement.
      </p>
      <div className="flex gap-2">
        <Button variant="destructive" size="sm" autoFocus disabled={remove.isPending} onClick={() => remove.mutate()}>
          {remove.isPending ? 'Suppression…' : 'Supprimer définitivement'}
        </Button>
        <Button variant="ghost" size="sm" disabled={remove.isPending} onClick={() => setConfirming(false)}>
          Annuler
        </Button>
      </div>
      <QueryError error={remove.error} />
    </div>
  )
}
