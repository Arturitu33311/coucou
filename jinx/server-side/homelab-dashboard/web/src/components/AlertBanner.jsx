import { SECTIONS } from '../routes/servicios-registry'

const CRITICAL = ['hermes', 'docker', 'openrouter']

/** Servicios críticos caídos: [{ name, detail }]. */
export function useAlerts(state) {
  if (!state) return []
  const alerts = []
  for (const sec of SECTIONS) {
    for (const item of sec.items) {
      if (!CRITICAL.includes(item.key)) continue
      const { status, detail } = item.derive(state)
      if (status === 'error') alerts.push({ name: item.name, detail })
    }
  }
  return alerts
}

/** Franja pegada a la píldora de navegación (vive dentro del header fijo de TopBar,
 *  así que ambos se mueven como una sola pieza y nunca se superponen). */
export default function AlertBanner({ alerts }) {
  if (!alerts?.length) return null
  return (
    <div
      role="alert"
      className="glass-pill flex w-full max-w-[1200px] items-start gap-3 rounded-3xl border border-status-error/40 bg-status-error/15 px-6 py-2.5 shadow-2xl"
    >
      <span className="mt-1.5 h-2 w-2 shrink-0 rounded-full bg-status-error shadow-glow-error" />
      <div className="flex flex-wrap gap-x-5 gap-y-1 font-label-xs text-label-xs text-status-error">
        {alerts.map(a => (
          <span key={a.name} className="flex flex-wrap items-center gap-x-1.5">
            <strong>{a.name}</strong>
            <span className="text-red-300/70">{a.detail}</span>
          </span>
        ))}
      </div>
    </div>
  )
}
