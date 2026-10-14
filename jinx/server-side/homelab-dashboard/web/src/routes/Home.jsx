import { lazy, Suspense, useCallback, useState } from 'react'
import {
  Activity,
  Bot,
  Boxes,
  Building2,
  Brain,
  Cpu,
  Gauge,
  HardDrive,
  Heart,
  Home,
  MemoryStick,
  Share2,
  ShieldAlert,
  Terminal,
  Wallet,
} from 'lucide-react'
import { Link } from 'react-router-dom'
import StatusBar from '../components/StatusBar'
import PlataformasWidget from '../components/PlataformasWidget'
import Page from '../components/Page'
import Section from '../components/Section'
import SwipeDeck from '../components/SwipeDeck'
import ServiceCard, { Metric, ProgressBar } from '../components/ServiceCard'
import OpenRouterTres from '../components/OpenRouterTres'
import GatewayRestart from '../components/GatewayRestart'
import TeamsLogin from '../components/TeamsLogin'

// estados de Docker en español
const ESTADO = { running: 'en marcha', stopped: 'detenido', starting: 'arrancando', healthy: 'sano', unhealthy: 'con fallos', starting: 'arrancando', exited: 'detenido', restarting: 'reiniciando', paused: 'en pausa', created: 'creado', dead: 'muerto' }
import ClaudeUsage from '../components/ClaudeUsage'
import { orStatus as orStatusOf } from '../lib/openrouter'
// three.js + 3d-force-graph pesan ~1.2 MB: carga diferida, el grafo está bajo el fold.
const CognitionGraph = lazy(() => import('../components/CognitionGraph'))
import { useDashboard, svc } from '../lib/api'
import { gb, int, loadStatus, pct, usd } from '../lib/format'

/* FASE 3 — cada tarjeta cableada a /api/state (100.92.254.125:8890 vía proxy /api). */

const findC = (docker, name) => docker.data?.containers?.find((c) => c.name === name)

export default function HomePage() {
  const { state, error, fetchedAt } = useDashboard()
  const [graphCounts, setGraphCounts] = useState(null)
  const onCounts = useCallback((c) => setGraphCounts(c), [])

  const hermes = svc(state, 'hermes')
  const mnem = svc(state, 'mnemosyne')
  const or = svc(state, 'openrouter')
  const cu = svc(state, 'claude_usage')
  const ollama = svc(state, 'ollama')
  const vga = svc(state, 'vga')
  const pixel = svc(state, 'pixel')
  const jinx = svc(state, 'vtuber')
  const docker = svc(state, 'docker')
  const ha = svc(state, 'home_assistant')
  const cpu = svc(state, 'glances_quick')
  const glslow = svc(state, 'glances_slow')
  const pihole = svc(state, 'pihole')

  const loading = !state && !error

  const hPlatforms = hermes.data
    ? Object.values(hermes.data.platforms || {}).filter((s) => s === 'connected').length
    : null

  const orStatus = orStatusOf(or)

  const cpuPct = cpu.data?.cpu ?? null
  const pkgTemp = glslow.data?.sensors?.find((s) => s.label === 'Package id 0')?.value ?? null
  const memPct = glslow.data?.mem?.percent ?? null
  const memUsed = glslow.data?.mem?.used ?? null
  const memTotal = glslow.data?.mem?.total ?? null
  const rootFs = glslow.data?.fs?.find((f) => f.mnt_point === '/')
  const diskPct = rootFs?.percent ?? null
  const emo = jinx.data?.dimensions

  const cpuStat = loadStatus(cpuPct, 75, 90)
  const memStat = loadStatus(memPct)
  const diskStat = loadStatus(diskPct, 80, 92)
  const tone = (s) => (s === 'error' ? 'bg-status-error' : s === 'warning' ? 'bg-status-warning' : 'bg-emerald-accent')

  return (
    <Page>
      <div className="flex w-full flex-col gap-8">
        <div className="flex w-full items-end justify-between border-b border-surface-variant pb-4 max-sm:flex-col max-sm:items-start max-sm:gap-3">
          <div>
            <h1 className="mb-2 font-display-lg text-display-lg tracking-tight text-on-surface max-md:text-[24px]">
              Homelab
            </h1>
            <p className="font-body-base text-body-base text-on-surface-variant">
              Infraestructura de Hermes Agent y el servidor — un vistazo.
            </p>
          </div>
          <div
            className={`flex shrink-0 items-center gap-2 rounded-full px-3 py-1.5 ${
              error ? 'bg-error-container/20 text-error' : 'bg-secondary-container/20 text-secondary'
            }`}
          >
            <span className={`h-1.5 w-1.5 rounded-full status-pulse ${error ? 'bg-error' : 'bg-secondary'}`} />
            <span className="font-label-xs text-label-xs ">
              {error
                ? 'agregador sin responder'
                : loading
                  ? 'conectando…'
                  : `actualizado ${fetchedAt ? new Date(fetchedAt).toLocaleTimeString('es-MX', { hour12: false }) : ''}`}
            </span>
          </div>
        </div>

        <StatusBar state={state} />

        <SwipeDeck labels={['Infraestructura IA', 'Casa y red', 'Monitoreo de servidor']}>
        <Section title="Infraestructura IA">
          <ServiceCard
            icon={Activity}
            title="Hermes Gateway"
            stale={hermes.stale}
            status={hermes.ok ? (hermes.data?.readiness === 'ok' ? 'healthy' : 'warning') : 'error'}
          >
            <Metric
              label="Estado"
              value={ESTADO[hermes.data?.gateway_state] ?? hermes.data?.gateway_state ?? '—'}
              sub={hermes.data ? `v${hermes.data.version} · ${hPlatforms}/6 plataformas` : null}
            />
            <div className="grid grid-cols-2 gap-4">
              <Metric label="Sesiones hoy" value={int(hermes.data?.sessions_today)} sub={`${int(hermes.data?.sessions_active)} act. 24h`} />
              <Metric label="Coste hoy" value={usd(hermes.data?.cost_today_usd)} />
            </div>
            <GatewayRestart />
          </ServiceCard>

          <TeamsLogin />

          <ServiceCard icon={Brain} title="Mnemosyne" accent="text-tertiary" stale={mnem.stale} status={mnem.ok ? 'healthy' : 'error'}>
            <div className="grid grid-cols-2 gap-4">
              <Metric label="Memorias" value={int(mnem.data?.memories)} sub={`+${int(mnem.data?.memories_today)} hoy`} />
              <Metric label="Hechos" value={int(mnem.data?.facts)} sub={`${int(mnem.data?.triples)} relaciones`} />
            </div>
          </ServiceCard>

          <ServiceCard icon={Wallet} title="OpenRouter" accent="text-status-warning" stale={or.stale} status={orStatus}>
            <OpenRouterTres or={or} />
          </ServiceCard>

          <ServiceCard icon={Terminal} title="Claude Code" accent="text-primary" stale={cu.stale} status={cu.ok ? ((cu.data?.limits?.five_hour?.pct ?? 0) >= 90 || (cu.data?.limits?.seven_day?.pct ?? 0) >= 90 ? 'warning' : 'healthy') : 'error'}>
            <ClaudeUsage cu={cu} />
          </ServiceCard>

          <ServiceCard
            icon={Bot}
            title="Ollama"
            accent="text-on-surface-variant"
            stale={ollama.stale}
            status={ollama.ok ? (ollama.data?.loaded?.length ? 'healthy' : 'idle') : 'error'}
          >
            <div className="grid grid-cols-2 gap-4">
              <Metric label="Modelos" value={int(ollama.data?.installed?.length)} sub={`v${ollama.data?.version ?? '—'}`} />
              <Metric label="Cargado ahora" value={ollama.data?.loaded?.length ? ollama.data.loaded[0].name : 'ninguno'} />
            </div>
          </ServiceCard>

          <ServiceCard icon={Gauge} title="VGA Panel" accent="text-secondary" stale={vga.stale} status={vga.ok ? 'healthy' : 'error'}>
            <div className="grid grid-cols-2 gap-4">
              <Metric
                label={`Clima ${vga.data?.clima?.ciudad ?? 'Zapopan'}`}
                value={vga.data?.clima?.temp_actual_c ?? '—'}
                unit="°C"
                sub={vga.data?.clima?.descripcion_actual}
              />
              <Metric label="Disco libre" value={vga.data?.disco?.libre_gb != null ? `${vga.data.disco.libre_gb}` : '—'} unit="GB" />
            </div>
          </ServiceCard>

          <Link to="/oficina" className="contents">
            <ServiceCard icon={Building2} title="Oficina de agentes" accent="text-secondary" stale={pixel.stale} status={pixel.ok ? 'healthy' : 'error'}>
              <Metric
                label="Agentes"
                value={pixel.ok ? int(pixel.data?.count) : '—'}
                sub={pixel.ok ? (pixel.data?.agents?.map((a) => a.name).join(' · ') || 'nadie en la oficina') : (pixel.error ?? 'sin dato')}
              />
              <span className="font-label-xs text-label-xs text-outline">Pixel Agents →</span>
            </ServiceCard>
          </Link>

          <Link to="/catos" className="contents">
            <ServiceCard icon={Heart} title="Jinx · emoción" accent="text-primary" stale={jinx.stale} status={jinx.ok ? 'healthy' : 'error'}>
              <div className="grid grid-cols-2 gap-4">
                <Metric label="Estado" value={jinx.data?.dominant_now ?? jinx.data?.current ?? '—'} sub={emo ? `score ${emo.emotion_score}` : null} />
                <Metric label="Afecto / confianza" value={emo ? `${Math.round(emo.affection)}` : '—'} sub={emo ? `${Math.round(emo.trust)} confianza` : null} />
              </div>
              <span className="font-label-xs text-label-xs text-outline">Cat OS →</span>
            </ServiceCard>
          </Link>

          <ServiceCard icon={Share2} title="Grafo de Conocimiento" size="lg" status="healthy">
            <div className="grid gap-6 lg:grid-cols-[minmax(0,300px)_1fr] lg:items-stretch">
              <div className="flex flex-col gap-4">
                <p className="font-body-base text-body-base text-on-surface-variant">
                  Clustering semántico y relaciones entre entidades (Cognee). Esfera de fuerzas 3D calculada en el navegador; color por grado de conexión.
                </p>
                <div className="grid grid-cols-2 gap-4">
                  <Metric label="Nodos" value={int(graphCounts?.nodes)} />
                  <Metric label="Edges" value={int(graphCounts?.links)} />
                </div>
                <Link to="/grafo" className="font-label-xs text-label-xs text-primary hover:underline">
                  Abrir explorador →
                </Link>
              </div>
              <div className="h-[440px] min-w-0 overflow-hidden max-md:h-[300px]">
                <Suspense
                  fallback={
                    <div className="flex h-full w-full items-center justify-center rounded-lg border border-white/10 bg-black/20 font-label-xs text-label-xs text-outline">
                      Cargando grafo…
                    </div>
                  }
                >
                  <CognitionGraph onCounts={onCounts} />
                </Suspense>
              </div>
            </div>
          </ServiceCard>
        </Section>

        <Section title="Casa y red">
          <ServiceCard icon={Home} title="Home Assistant" accent="text-secondary" stale={ha.stale} status={ha.ok ? 'healthy' : 'error'}>
            <div className="grid grid-cols-2 gap-4">
              <Metric label="Entidades" value={int(ha.data?.entities)} sub={ha.data ? `${ha.data.by_domain?.sensor ?? 0} sensores` : null} />
              <Metric label="person.alan" value={ha.data?.key_states?.['person.alan'] ?? '—'} />
            </div>
            <Metric label="Clima / sol" value={ha.data?.key_states?.['weather.forecast_casa'] ?? '—'} sub={ha.data?.key_states?.['sun.sun']} />
          </ServiceCard>

          <ServiceCard icon={ShieldAlert} title="Pi-hole" accent="text-status-error" stale={pihole.stale} status={pihole.data?.status === 'ok' ? 'healthy' : 'error'}>
            <Metric
              label="Estado"
              value={pihole.data?.status ?? (pihole.ok === false ? 'degraded' : '—')}
              sub={pihole.data?.reason ?? pihole.error ?? 'contenedor unhealthy'}
            />
          </ServiceCard>
        </Section>

        <Section title="Monitoreo de servidor">
          <ServiceCard icon={Cpu} title="CPU" stale={cpu.stale} status={cpuStat}>
            <Metric
              label={cpu.data?.cpu_name?.replace(/\(.*?\)/g, '').replace(/CPU.*/, '').trim() || 'CPU'}
              value={pct(cpuPct)}
              unit="%"
              sub={pkgTemp != null ? `${pkgTemp} °C` : null}
            />
            <ProgressBar value={cpuPct ?? 0} tone={tone(cpuStat)} />
          </ServiceCard>

          <ServiceCard icon={MemoryStick} title="Memoria" accent={memStat === 'healthy' ? 'text-primary' : 'text-status-warning'} stale={glslow.stale} status={memStat}>
            <Metric label="RAM" value={pct(memPct)} unit="%" sub={memUsed ? `${gb(memUsed)} / ${gb(memTotal)} GB` : null} />
            <ProgressBar value={memPct ?? 0} tone={tone(memStat)} />
          </ServiceCard>

          <ServiceCard icon={HardDrive} title="Disco" accent={diskStat === 'healthy' ? 'text-primary' : 'text-status-warning'} stale={glslow.stale} status={diskStat}>
            <Metric label="/ (ext4)" value={pct(diskPct)} unit="%" sub={rootFs ? `${gb(rootFs.used)} / ${gb(rootFs.size)} GB` : null} />
            <ProgressBar value={diskPct ?? 0} tone={tone(diskStat)} />
          </ServiceCard>

          <ServiceCard icon={Boxes} title="Contenedores" size="lg" stale={docker.stale} status={docker.ok ? 'healthy' : 'error'}>
            <div className="mb-2 flex gap-6">
              <Metric label="Uptime servidor" value={glslow.data?.uptime?.split(',')[0] ?? '—'} />
              <Metric label="Contenedores" value={int(docker.data?.containers?.length)} />
              <Metric
                label="Con problemas"
                value={int((docker.data?.containers ?? []).filter((c) => c.health === 'unhealthy' || c.state !== 'running').length)}
              />
            </div>
            <div className="grid grid-cols-1 gap-x-8 gap-y-2 sm:grid-cols-2 lg:grid-cols-3">
              {(docker.data?.containers ?? []).map((c) => {
                const bad = c.health === 'unhealthy' || c.state !== 'running'
                return (
                  <div key={c.name} className="flex items-center justify-between gap-3 border-b border-white/5 py-2">
                    <div className="flex min-w-0 items-center gap-2">
                      <span className={`h-1.5 w-1.5 shrink-0 rounded-full ${bad ? 'bg-status-error' : 'bg-status-healthy'}`} />
                      <span className="truncate font-body-base text-body-base text-on-surface-variant">{c.name}</span>
                    </div>
                    <span className={`shrink-0 font-label-xs text-label-xs  ${bad ? 'text-status-error' : 'text-outline'}`}>
                      {ESTADO[c.health && c.health !== 'none' ? c.health : c.state] ?? (c.health && c.health !== 'none' ? c.health : c.state)}
                    </span>
                  </div>
                )
              })}
              {!docker.data && (
                <span className="font-label-xs text-label-xs text-outline">cargando…</span>
              )}
            </div>
          </ServiceCard>
        </Section>
        </SwipeDeck>

        <div className="grid grid-cols-1 gap-4 md:grid-cols-6 lg:grid-cols-12">
          <PlataformasWidget />
        </div>
      </div>
    </Page>
  )
}
