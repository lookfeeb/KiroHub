import { lazy, LazyExoticComponent, ComponentType } from 'react'
import { Home, Key, Settings, Network, MessageSquare, LucideIcon } from 'lucide-react'

export interface RouteConfig {
  id: string;
  icon: LucideIcon;
  nameKey: string;
  descKey?: string;
  component: LazyExoticComponent<ComponentType<any>>;
}

// 路由配置：菜单项 + 懒加载组件
export const routes: RouteConfig[] = [
  { id: 'home', icon: Home, nameKey: 'nav.home', component: lazy(() => import('./components/features/Home/index')) },
  { id: 'accounts', icon: Key, nameKey: 'nav.accounts', component: lazy(() => import('./components/features/AccountManager/index')) },
  { id: 'sessions', icon: MessageSquare, nameKey: 'nav.sessions', component: lazy(() => import('./components/features/SessionManager/index')) },
  { id: 'gateway', icon: Network, nameKey: 'nav.gateway', component: lazy(() => import('./components/features/Gateway/index')) },
  { id: 'settings', icon: Settings, nameKey: 'nav.settings', component: lazy(() => import('./components/features/Settings/index')) },
]

// 内部路由（不在侧边栏显示）
export const internalRoutes: Record<string, LazyExoticComponent<ComponentType<any>>> = {
  callback: lazy(() => import('./components/shared/AuthCallback'))
}
