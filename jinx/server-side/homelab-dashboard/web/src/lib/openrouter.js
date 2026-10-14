import { usd } from './format'

/** Los valores de OpenRouter que se ven idénticos en todo el dashboard:
 *  cuenta (dinero real) y límite de Jinx. Vienen ya calculados
 *  del agregador (openrouter_saldo.tres_valores). Umbrales = los del watchdog. */
const lvl = (v, err, warn) => (v == null ? 'idle' : v < err ? 'error' : v < warn ? 'warning' : 'healthy')

export function orTres(o) {
  const d = o?.data
  if (!d || d.cuenta === undefined) return null
  const row = (key, label, sub, restante, reset, limite, err, warn) => ({
    key, label, restante, limite, reset, status: lvl(restante, err, warn),
    sub: limite == null ? 'sin límite' : `de ${usd(limite)} ${{ weekly: 'semanal', monthly: 'mensual', daily: 'diario' }[reset] ?? ''}`.trim(),
  })
  return [
    { key: 'cuenta', label: 'Créditos en cuenta', restante: d.cuenta, sub: 'saldo real de OpenRouter', status: lvl(d.cuenta, 2, 5) },
    row('jinx', 'Límite de Jinx', null, d.jinx?.restante, d.jinx?.reset, d.jinx?.limite, 0.5, 2),
  ]
}

const ORDER = { idle: 0, healthy: 1, warning: 2, error: 3 }
/** Alerta general = solo créditos reales de la cuenta. El límite de Jinx (fila
 *  'jinx') es un tope de la llave, no dinero: se queda sin límite y sigue
 *  habiendo saldo de sobra, así que no debe pintar la app de rojo ni disparar
 *  el banner. */
export function orStatus(o) {
  const t = orTres(o)
  if (!t) return 'idle'
  return t.filter(r => r.key !== 'jinx').reduce((w, r) => (ORDER[r.status] > ORDER[w] ? r.status : w), 'healthy')
}

/** "cuenta $0.81 · Jinx $0.00" para listas y avisos. */
export function orResumen(o) {
  const t = orTres(o)
  return t ? `cuenta ${usd(t[0].restante)} · Jinx ${usd(t[1].restante)}` : o?.error
}
