import { useState, useRef, useEffect, useCallback, useMemo } from 'react'
import { Search, FileDown, RefreshCcw, RotateCw, Trash2, ArrowUp, ArrowDown, X, TrendingUp, Clock, Calendar, CheckSquare, Square, Sparkles, LayoutGrid, List, Edit, Check } from 'lucide-react'
import { useApp } from '../../../hooks/useApp'
import FilterDropdown from './FilterDropdown'
import { getThemeAccent } from '../KiroConfig/themeAccent'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'

interface AccountHeaderProps {
  searchTerm: string;
  onSearchChange: (value: string) => void;
  selectedCount: number;
  onBatchDelete: () => void;
  onBatchEdit: () => void;
  onImport: () => void;
  onExport: () => void;
  onRefresh: () => void;
  onRefreshAll: () => void;
  autoRefreshing: boolean;
  refreshProgress: { current: number; total: number };
  allGroups?: any[];
  selectedGroup: any;
  onGroupFilter: (group: any) => void;
  allTags?: any[];
  selectedTag: any;
  onTagFilter: (tag: any) => void;
  selectedStatus: any;
  onStatusFilter: (status: any) => void;
  sortBy?: string;
  onSortChange: (sort: string) => void;
  viewMode?: string;
  onViewModeChange: (mode: string) => void;
  advancedFilters?: any;
  onAdvancedFiltersChange: (filters: any) => void;
  totalCount?: number;
  onSelectAll: (checked?: boolean) => void;
  onDeselectAll: () => void;
}

/**
 * 紧凑工具按钮：32x32，统一图标按钮样式。
 * 选中态用 accent 实色渐变，普通态用 glass-card border。
 */
interface IconButtonProps {
  onClick: () => void;
  active?: boolean;
  disabled?: boolean;
  title: string;
  ariaLabel?: string;
  accent: any;
  badge?: React.ReactNode;
  className?: string;
  children: React.ReactNode;
}

function IconButton({ onClick, active, disabled, title, ariaLabel, accent, badge, className = '', children }: IconButtonProps) {
  const base = 'relative h-8 w-8 rounded-md flex items-center justify-center transition-colors disabled:opacity-50 cursor-pointer focus:outline-none focus:ring-2'
  const variant = active
    ? `${accent.solidBg} text-white shadow-sm`
    : 'glass-card border border-border hover:bg-muted/50 text-muted-foreground'
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          onClick={onClick}
          disabled={disabled}
          aria-label={ariaLabel || title}
          className={`${base} ${variant} ${accent.ring} ${className}`}
        >
          {children}
          {badge}
        </button>
      </TooltipTrigger>
      <TooltipContent>{title}</TooltipContent>
    </Tooltip>
  )
}

function AccountHeader({
  searchTerm,
  onSearchChange,
  selectedCount,
  onBatchDelete,
  onBatchEdit,
  onImport,
  onExport,
  onRefresh,
  onRefreshAll,
  autoRefreshing,
  refreshProgress,
  allGroups = [],
  selectedGroup,
  onGroupFilter,
  allTags = [],
  selectedTag,
  onTagFilter,
  selectedStatus,
  onStatusFilter,
  sortBy = 'default',
  onSortChange,
  viewMode = 'card',
  onViewModeChange,
  advancedFilters = {},
  onAdvancedFiltersChange,
  totalCount = 0,
  onSelectAll,
  onDeselectAll,
}: AccountHeaderProps) {
  const { t, theme } = useApp()
  const accent = useMemo(() => getThemeAccent(theme), [theme])

  const [searchExpanded, setSearchExpanded] = useState(false)
  const [refreshSpin, setRefreshSpin] = useState(false)
  const [refreshDone, setRefreshDone] = useState(false)
  const [localSearchTerm, setLocalSearchTerm] = useState(searchTerm)
  const searchRef = useRef<HTMLDivElement>(null)
  const debounceTimerRef = useRef<NodeJS.Timeout | null>(null)

  // 搜索防抖
  const handleSearchChange = useCallback((value: string) => {
    setLocalSearchTerm(value)
    if (debounceTimerRef.current) clearTimeout(debounceTimerRef.current)
    debounceTimerRef.current = setTimeout(() => onSearchChange(value), 300)
  }, [onSearchChange])

  useEffect(() => {
    return () => {
      if (debounceTimerRef.current) clearTimeout(debounceTimerRef.current)
    }
  }, [])

  useEffect(() => {
    setLocalSearchTerm(searchTerm)
  }, [searchTerm])

  // 点击外部关闭搜索框
  useEffect(() => {
    const handleClick = (e: MouseEvent) => {
      if (searchExpanded && searchRef.current && !searchRef.current.contains(e.target as Node) && !localSearchTerm) {
        setSearchExpanded(false)
      }
    }
    document.addEventListener('mousedown', handleClick)
    return () => document.removeEventListener('mousedown', handleClick)
  }, [localSearchTerm, searchExpanded])

  const sortOptions = [
    { key: 'usage', label: t('sort.usage'), icon: TrendingUp },
    { key: 'added', label: t('sort.added'), icon: Clock },
    { key: 'trial', label: t('sort.trial'), icon: Calendar },
  ]

  const isBatchMode = selectedCount > 0

  return (
    <div className="border-b border-border bg-card/30 backdrop-blur-sm px-5 py-3">
      <div className="flex items-center justify-between gap-3">
        {/* 左侧：标题 / 选中提示 */}
        <div className="flex items-center gap-2.5 min-w-0">
          <div className={`w-10 h-10 rounded-xl bg-gradient-to-br ${accent.gradientFrom} ${accent.gradientTo} flex items-center justify-center shadow-md ring-1 ring-primary/20 flex-shrink-0`}>
            <Sparkles size={20} className="text-primary-foreground" />
          </div>
          <div className="flex flex-col min-w-0">
            {isBatchMode ? (
              <>
                <h1 className="text-sm font-semibold text-foreground leading-tight">
                  {t('common.selected')} {selectedCount} {t('accounts.title')}
                </h1>
                <p className="text-[11px] text-muted-foreground leading-tight">批量操作模式</p>
              </>
            ) : (
              <>
                <h1 className="text-sm font-semibold text-foreground leading-tight truncate">{t('accounts.title')}</h1>
                <p className="text-[11px] text-muted-foreground leading-tight truncate">{t('accounts.subtitle')}</p>
              </>
            )}
          </div>
        </div>

        {/* 右侧：搜索 + 操作 */}
        <div className="flex items-center gap-1.5 flex-shrink-0">
          {!isBatchMode && (
            <>
              {/* 搜索框 - 可收缩 */}
              <div ref={searchRef} className="relative">
                {searchExpanded || localSearchTerm ? (
                  <div className="relative">
                    <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground" size={13} />
                    <input
                      type="text"
                      placeholder={t('accounts.search')}
                      value={localSearchTerm}
                      onChange={(e) => handleSearchChange(e.target.value)}
                      autoFocus
                      className={`pl-8 pr-8 h-8 bg-muted/40 border border-transparent rounded-md text-xs w-44 focus:outline-none focus:ring-2 ${accent.ring} text-foreground transition-all`}
                    />
                    {localSearchTerm && (
                      <button
                        onClick={() => {
                          setLocalSearchTerm('')
                          onSearchChange('')
                        }}
                        className="absolute right-1.5 top-1/2 -translate-y-1/2 p-1 rounded hover:bg-muted/50 transition-colors cursor-pointer"
                        title="清空"
                      >
                        <X size={12} className="text-muted-foreground" />
                      </button>
                    )}
                  </div>
                ) : (
                  <IconButton
                    onClick={() => setSearchExpanded(true)}
                    title={t('accounts.search')}
                    accent={accent}
                  >
                    <Search size={14} />
                  </IconButton>
                )}
              </div>

              {/* 排序按钮组 */}
              <div className="flex gap-1 border border-border rounded-md p-0.5 bg-card/40">
                {sortOptions.map(({ key, label, icon: Icon }) => {
                  const isActive = sortBy.startsWith(key)
                  const isDesc = sortBy.endsWith('Desc')
                  return (
                    <IconButton
                      key={key}
                      onClick={() => {
                        if (isActive) {
                          onSortChange(isDesc ? `${key}Asc` : 'default')
                        } else {
                          onSortChange(`${key}Desc`)
                        }
                      }}
                      active={isActive}
                      title={label}
                      accent={accent}
                      className="h-7 w-7 border-0"
                      badge={isActive && (
                        <div className="absolute -top-1 -right-1 w-3.5 h-3.5 rounded-full bg-background shadow flex items-center justify-center ring-1 ring-border">
                          {isDesc ? <ArrowDown size={9} className="text-foreground" /> : <ArrowUp size={9} className="text-foreground" />}
                        </div>
                      )}
                    >
                      <Icon size={13} />
                    </IconButton>
                  )
                })}
              </div>
            </>
          )}

          {/* 批量操作 */}
          {isBatchMode && (
            <>
              <IconButton
                onClick={() => onSelectAll()}
                title="全选"
                accent={accent}
              >
                <CheckSquare size={14} className={accent.text} />
              </IconButton>
              <IconButton
                onClick={onDeselectAll}
                title="取消全选"
                accent={accent}
              >
                <Square size={14} />
              </IconButton>
              <button
                onClick={onBatchEdit}
                className={`px-3 h-8 text-xs font-medium rounded-md inline-flex items-center gap-1.5 cursor-pointer text-white shadow-sm bg-gradient-to-br ${accent.gradientFrom} ${accent.gradientTo} hover:opacity-90 transition-opacity`}
                title="批量编辑（标签和分组）"
              >
                <Edit size={13} />
                批量编辑 ({selectedCount})
              </button>
              <button
                onClick={onBatchDelete}
                className="px-3 h-8 text-xs font-medium rounded-md inline-flex items-center gap-1.5 cursor-pointer text-white shadow-sm bg-gradient-to-r from-red-500 to-red-600 hover:opacity-90 transition-opacity"
                title={t('accounts.batchDelete')}
              >
                <Trash2 size={13} />
                ({selectedCount})
              </button>
            </>
          )}

          {/* 通用操作按钮组 */}
          <div className="flex gap-1 ml-1">
            <IconButton onClick={onExport} title={t('accounts.export')} accent={accent}>
              <FileDown size={14} className={accent.text} />
            </IconButton>
            <IconButton onClick={() => { setRefreshSpin(true); setRefreshDone(false); onRefresh(); setTimeout(() => { setRefreshSpin(false); setRefreshDone(true); setTimeout(() => setRefreshDone(false), 900) }, 600) }} title={t('accounts.refreshList')} accent={accent}>
              {refreshDone
                ? <Check size={14} className="text-green-500" />
                : <RotateCw size={14} className={refreshSpin ? 'animate-spin' : ''} />}
            </IconButton>
            <IconButton onClick={onRefreshAll} disabled={autoRefreshing} title={t('accounts.refreshAll')} accent={accent}>
              <RefreshCcw size={14} className={`${accent.text} ${autoRefreshing ? 'animate-spin' : ''}`} />
            </IconButton>
          </div>
        </div>
      </div>

      {/* 刷新进度条 */}
      {autoRefreshing && refreshProgress.total > 0 && (
        <div className="mt-2 flex items-center gap-2">
          <div className="flex-1 h-1 bg-muted/40 rounded-full overflow-hidden">
            <div
              className={`h-full bg-gradient-to-r ${accent.gradientFrom} ${accent.gradientTo} rounded-full transition-all`}
              style={{ width: `${(refreshProgress.current / refreshProgress.total) * 100}%` }}
            />
          </div>
          <span className={`text-[11px] font-medium ${accent.text}`}>
            {refreshProgress.current}/{refreshProgress.total}
          </span>
        </div>
      )}
    </div>
  )
}

export default AccountHeader
