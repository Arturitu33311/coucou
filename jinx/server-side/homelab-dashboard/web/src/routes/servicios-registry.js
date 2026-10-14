import { orStatus, orResumen } from '../lib/openrouter'
import {
  Activity, Brain, Wallet, Bot, Gauge, Heart, Share2,
  Home, ShieldAlert, Cpu, MemoryStick, HardDrive, Boxes, Building2, Images,
} from 'lucide-react'
import { svc } from '../lib/api'
import { loadStatus } from '../lib/format'

const findC = (docker, name) => docker.data?.containers?.find((c) => c.name === name)
const cStatus = (docker, name) => {
  const c = findC(docker, name)
  if (!c) return { status: 'idle', detail: 'sin contenedor' }
  const bad = c.health === 'unhealthy' || c.state !== 'running'
  return { status: bad ? 'error' : 'healthy', detail: c.status || c.state }
}

/**
 * Directorio real de servicios. `url` = lo navegable desde Tailscale
 * (100.92.254.125 / server); null = sin UI remota (Ollama, VGA solo localhost).
 * Ver FASE-A-AUDITORIA-v2.md §7.
 */
export const SECTIONS = [
  {
    title: 'Infraestructura IA',
    items: [
      {
        key: 'hermes', name: 'Hermes Gateway', icon: Activity,
        desc: 'Router de sesiones y modelos de Jinx (Hermes Agent).',
        url: 'http://100.92.254.125:8642',
        derive: (s) => {
          const h = svc(s, 'hermes')
          return {
            status: h.ok ? (h.data?.readiness === 'ok' ? 'healthy' : 'warning') : 'error',
            detail: h.data ? `v${h.data.version} · ${h.data.gateway_state}` : h.error,
          }
        },
      },
      {
        key: 'mnemosyne', name: 'Mnemosyne', icon: Brain,
        desc: 'Memoria de largo plazo: memorias, facts y triples.',
        url: null,
        derive: (s) => {
          const m = svc(s, 'mnemosyne')
          return { status: m.ok ? 'healthy' : 'error', detail: m.data ? `${m.data.memories} memorias` : m.error }
        },
      },
      {
        key: 'openrouter', name: 'OpenRouter', icon: Wallet,
        desc: 'Proveedor LLM: créditos de la cuenta y límite de Jinx.',
        url: 'https://openrouter.ai/activity',
        derive: (s) => {
          const o = svc(s, 'openrouter')
          return { status: orStatus(o), detail: orResumen(o) }
        },
      },
      {
        key: 'ollama', name: 'Ollama', icon: Bot,
        desc: 'Inferencia local de modelos (solo localhost, sin UI remota).',
        url: null,
        derive: (s) => {
          const o = svc(s, 'ollama')
          return {
            status: o.ok ? (o.data?.loaded?.length ? 'healthy' : 'idle') : 'error',
            detail: o.data ? `${o.data.installed?.length ?? 0} modelos · v${o.data.version}` : o.error,
          }
        },
      },
      {
        key: 'vtuber', name: 'Jinx · Live2D + emoción', icon: Heart,
        desc: 'Avatar Live2D, wakeword y estado emocional (Open-LLM-VTuber).',
        url: 'http://100.92.254.125:8910',
        derive: (s) => {
          const v = svc(s, 'vtuber')
          return { status: v.ok ? 'healthy' : 'error', detail: v.data?.dominant_now ? `estado: ${v.data.dominant_now}` : v.error }
        },
      },
      {
        key: 'pixel', name: 'Pixel Agents · Oficina', icon: Building2,
        desc: 'Oficina pixel-art con los agentes (Jinx + Claude Code) que corre en la laptop.',
        url: null,
        route: '/oficina',
        derive: (s) => {
          const p = svc(s, 'pixel')
          return {
            status: p.ok ? 'healthy' : 'error',
            detail: p.data ? `${p.data.count} agente(s): ${p.data.agents.map((a) => a.name).join(', ') || '—'}` : (p.error ?? 'laptop sin servidor'),
          }
        },
      },
      {
        key: 'vga', name: 'VGA Panel', icon: Gauge,
        desc: 'Panel personal de Alan (clima, saldo, disco). Solo localhost.',
        url: null,
        derive: (s) => {
          const v = svc(s, 'vga')
          return { status: v.ok ? 'healthy' : 'error', detail: v.data?.clima ? `${v.data.clima.ciudad} ${v.data.clima.temp_actual_c}°C` : v.error }
        },
      },
      {
        key: 'graph', name: 'Grafo de Conocimiento', icon: Share2,
        desc: 'Grafo semántico de Cognee (entidades y relaciones).',
        url: 'http://100.92.254.125:8902',
        route: '/grafo',
        derive: () => ({ status: 'healthy', detail: 'regenera de madrugada (04:40)' }),
      },
    ],
  },
  {
    title: 'Casa y red',
    items: [
      {
        key: 'home_assistant', name: 'Home Assistant', icon: Home,
        desc: 'Domótica de casa.',
        url: 'http://100.92.254.125:8123',
        derive: (s) => {
          const h = svc(s, 'home_assistant')
          return { status: h.ok ? 'healthy' : 'error', detail: h.data ? `${h.data.entities} entidades` : h.error }
        },
      },
      {
        // Ojo: `tailscale serve` enruta por nombre; por IP (100.92.254.125:2283) responde 404. Puerto 2283 cerrado a la LAN (127.0.0.1 en docker).
        key: 'immich', name: 'Immich', icon: Images,
        desc: 'Fotos y videos (4 contenedores). Solo tailnet: http://server:2283.',
        url: 'http://server:2283',
        derive: (s) => {
          const d = svc(s, 'docker')
          const srv = cStatus(d, 'immich_server')
          const caidos = ['immich_postgres', 'immich_machine_learning', 'immich_redis']
            .filter((n) => cStatus(d, n).status !== 'healthy').length
          if (srv.status !== 'healthy') return { status: 'error', detail: `servidor: ${srv.detail}` }
          return caidos
            ? { status: 'warning', detail: `servidor ok · ${caidos} de 3 servicios de apoyo con problemas` }
            : { status: 'healthy', detail: srv.detail }
        },
      },
      {
        key: 'pihole', name: 'Pi-hole', icon: ShieldAlert,
        desc: 'DNS y bloqueo de anuncios (contenedor flapea por carga).',
        url: 'http://100.92.254.125:8080/admin',
        derive: (s) => {
          const p = svc(s, 'pihole')
          return {
            status: p.data?.status === 'ok' ? 'healthy' : 'warning',
            detail: p.data?.reason ?? p.data?.status ?? 'degradado',
          }
        },
      },
    ],
  },
  {
    title: 'Monitoreo de servidor',
    items: [
      {
        key: 'cpu', name: 'CPU', icon: Cpu,
        desc: 'Uso por core y temperatura (Glances).',
        url: 'http://100.92.254.125:61208',
        route: '/sistema',
        derive: (s) => {
          const c = svc(s, 'glances_quick')
          return { status: loadStatus(c.data?.cpu, 75, 90), detail: c.data ? `${Math.round(c.data.cpu)}% · ${c.data.percpu?.length ?? 0} cores` : c.error }
        },
      },
      {
        key: 'mem', name: 'Memoria', icon: MemoryStick,
        desc: 'RAM y swap del servidor.',
        url: 'http://100.92.254.125:61208',
        route: '/sistema',
        derive: (s) => {
          const g = svc(s, 'glances_slow')
          const p = g.data?.mem?.percent
          return { status: loadStatus(p), detail: p != null ? `${Math.round(p)}% usado` : g.error }
        },
      },
      {
        key: 'disk', name: 'Disco', icon: HardDrive,
        desc: 'Uso del filesystem raíz.',
        url: 'http://100.92.254.125:61208',
        route: '/sistema',
        derive: (s) => {
          const g = svc(s, 'glances_slow')
          const root = g.data?.fs?.find((f) => f.mnt_point === '/')
          return { status: loadStatus(root?.percent, 80, 92), detail: root ? `${Math.round(root.percent)}% de / (ext4)` : g.error }
        },
      },
      {
        key: 'containers', name: 'Contenedores', icon: Boxes,
        desc: 'Docker: estado y stats por contenedor.',
        url: null,
        route: '/sistema',
        derive: (s) => {
          const d = svc(s, 'docker')
          const bad = (d.data?.containers ?? []).filter((c) => c.health === 'unhealthy' || c.state !== 'running').length
          return { status: bad ? 'warning' : d.ok ? 'healthy' : 'error', detail: d.data ? `${d.data.containers.length} activos · ${bad} con problemas` : d.error }
        },
      },
    ],
  },
]
