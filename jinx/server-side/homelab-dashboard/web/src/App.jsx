import { Routes, Route } from 'react-router-dom'
import Layout from './Layout'
import HomePage from './routes/Home'
import ServiciosPage from './routes/Servicios'
import GrafoPage from './routes/Grafo'
import SistemaPage from './routes/Sistema'
import CatOSPage from './routes/CatOS'
import PlataformasPage from './routes/Plataformas'
import JinxPage from './routes/Jinx'
import OficinaPage from './routes/Oficina'

export default function App() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route index element={<HomePage />} />
        <Route path="servicios" element={<ServiciosPage />} />
        <Route path="grafo" element={<GrafoPage />} />
        <Route path="sistema" element={<SistemaPage />} />
        <Route path="catos" element={<CatOSPage />} />
        <Route path="plataformas" element={<PlataformasPage />} />
        <Route path="jinx" element={<JinxPage />} />
        <Route path="oficina" element={<OficinaPage />} />
        <Route path="*" element={<HomePage />} />
      </Route>
    </Routes>
  )
}
