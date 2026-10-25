import { Route, Routes } from 'react-router-dom'
import { Layout } from '@/components/Layout'
import { CrawlPage } from '@/pages/CrawlPage'
import { UrlDetailPage } from '@/pages/UrlDetailPage'
import { ProjectPage } from '@/pages/ProjectPage'
import { ProjectsPage } from '@/pages/ProjectsPage'
import { LoginPage } from '@/pages/LoginPage'

export default function App() {
  return (
    <Routes>
      <Route path="login" element={<LoginPage />} />
      <Route element={<Layout />}>
        <Route index element={<ProjectsPage />} />
        <Route path="projects/:id" element={<ProjectPage />} />
        <Route path="crawls/:id" element={<CrawlPage />} />
        <Route path="crawls/:id/url" element={<UrlDetailPage />} />
        <Route path="*" element={<p className="text-sm text-muted-foreground">Page introuvable.</p>} />
      </Route>
    </Routes>
  )
}
