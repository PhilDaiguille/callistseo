import { CartesianGrid, Line, LineChart, PolarAngleAxis, RadialBar, RadialBarChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import { scoreColor } from '@/lib/score'

// Recharts pèse lourd : ce module n'est chargé qu'à la demande (voir index.tsx).
// Les couleurs sont des jetons CSS (`var(--…)`), donc suivent le thème clair / sombre.

/** Anneau du score (0-100). Purement décoratif : le chiffre est écrit par le parent. */
export function GaugeRing({ score, size }: { score: number; size: number }) {
  return (
    <RadialBarChart width={size} height={size} data={[{ value: score }]} innerRadius="80%" outerRadius="100%" startAngle={90} endAngle={-270} margin={{ top: 0, right: 0, bottom: 0, left: 0 }}>
      <PolarAngleAxis type="number" domain={[0, 100]} tick={false} axisLine={false} />
      <RadialBar dataKey="value" cornerRadius={size} fill={scoreColor(score)} background={{ fill: 'var(--secondary)' }} isAnimationActive={false} />
    </RadialBarChart>
  )
}

/** Évolution du score de santé, audit par audit. */
export function TrendLines({ points }: { points: { id: number; health: number; errors: number }[] }) {
  return (
    <ResponsiveContainer width="100%" height="100%">
      <LineChart data={points} margin={{ top: 8, right: 24, bottom: 0, left: -16 }}>
        <CartesianGrid stroke="var(--line)" vertical={false} />
        <XAxis dataKey="id" tickFormatter={(id: number) => `#${id}`} tick={{ fill: 'var(--muted-foreground)', fontSize: 12 }} axisLine={false} tickLine={false} />
        <YAxis domain={[0, 100]} ticks={[0, 50, 80, 100]} tick={{ fill: 'var(--muted-foreground)', fontSize: 12 }} axisLine={false} tickLine={false} />
        <Tooltip
          contentStyle={{ background: 'var(--popover)', border: '1px solid var(--line)', borderRadius: 12, color: 'var(--popover-foreground)', fontSize: 13 }}
          labelFormatter={(id) => `Audit #${id}`}
          formatter={(value, _name, item) => [`${value} / 100 · ${(item.payload as { errors: number }).errors} erreurs`, 'Score']}
        />
        <Line type="monotone" dataKey="health" stroke="var(--chart-info)" strokeWidth={2.5} dot={{ r: 3, fill: 'var(--chart-info)' }} isAnimationActive={false} />
      </LineChart>
    </ResponsiveContainer>
  )
}
