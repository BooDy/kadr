import React, { useEffect, useState } from 'react';
import {
  Folder,
  X,
  ChevronRight,
  ArrowUp,
  Compass,
  HardDrive,
  Loader2,
  AlertCircle,
} from 'lucide-react';
import { api } from '../../api/client';
import type { FsBrowseResponse } from '../../types';

export interface FolderPickerModalProps {
  isOpen: boolean;
  initialPath?: string;
  onClose: () => void;
  onSelect: (path: string) => void;
}

export const FolderPickerModal: React.FC<FolderPickerModalProps> = ({
  isOpen,
  initialPath,
  onClose,
  onSelect,
}) => {
  const [currentPath, setCurrentPath] = useState<string>(initialPath || '');
  const [browseData, setBrowseData] = useState<FsBrowseResponse | null>(null);
  const [isLoading, setIsLoading] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);

  const fetchPath = async (targetPath?: string) => {
    setIsLoading(true);
    setError(null);
    try {
      const res = await api.browseFilesystem(targetPath || undefined);
      setBrowseData(res);
      setCurrentPath(res.current_path);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Failed to browse filesystem';
      setError(msg);
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    if (isOpen) {
      const initial = initialPath || '';
      setCurrentPath(initial);
      fetchPath(initial);
    } else {
      setBrowseData(null);
      setError(null);
    }
  }, [isOpen, initialPath]);

  if (!isOpen) return null;

  const handleNavigate = (path: string) => {
    fetchPath(path);
  };

  const handleSelectCurrent = () => {
    if (currentPath) {
      onSelect(currentPath);
      onClose();
    }
  };

  // Parse path breadcrumbs
  const getBreadcrumbs = () => {
    if (!currentPath || currentPath === '/') {
      return [{ name: '/', path: '/' }];
    }
    const parts = currentPath.split('/').filter(Boolean);
    const crumbs: { name: string; path: string }[] = [{ name: '/', path: '/' }];
    let accumulated = '';
    for (const part of parts) {
      accumulated += `/${part}`;
      crumbs.push({ name: part, path: accumulated });
    }
    return crumbs;
  };

  const breadcrumbs = getBreadcrumbs();

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Select Server Folder"
      className="fixed inset-0 z-50 bg-black/80 backdrop-blur-sm flex items-center justify-center p-4"
    >
      <div className="bg-panel border border-border-subtle rounded-2xl max-w-xl w-full p-6 shadow-2xl flex flex-col max-h-[90vh]">
        {/* Header */}
        <div className="flex items-center justify-between pb-4 border-b border-border-subtle">
          <div className="flex items-center gap-2">
            <HardDrive className="w-5 h-5 text-accent" />
            <h3 className="text-lg font-bold text-text-main">Select Server Folder</h3>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close folder picker"
            className="p-1.5 text-muted hover:text-text-main hover:bg-panel-hover rounded-lg transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Shortcuts */}
        {browseData?.shortcuts && browseData.shortcuts.length > 0 && (
          <div className="pt-3 pb-2 flex flex-wrap items-center gap-1.5">
            <span className="text-xs text-muted flex items-center gap-1 mr-1">
              <Compass className="w-3.5 h-3.5 text-accent/80" /> Shortcuts:
            </span>
            {browseData.shortcuts.map((sc) => (
              <button
                key={sc.path}
                type="button"
                onClick={() => handleNavigate(sc.path)}
                className={`px-2.5 py-1 text-xs rounded-lg transition-colors cursor-pointer border focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-highlight ${
                  currentPath === sc.path
                    ? 'bg-accent/20 border-accent/40 text-accent font-semibold'
                    : 'bg-canvas/60 hover:bg-canvas border-border-subtle text-muted hover:text-text-main'
                }`}
              >
                {sc.name}
              </button>
            ))}
          </div>
        )}

        {/* Breadcrumb Path Bar */}
        <div className="mt-2 mb-3 bg-canvas/80 border border-border-subtle rounded-xl p-2.5 flex items-center gap-1.5 overflow-x-auto text-xs font-mono">
          {browseData?.parent_path !== null && browseData?.parent_path !== undefined && (
            <button
              type="button"
              onClick={() => handleNavigate(browseData.parent_path!)}
              aria-label="Up one folder"
              title={`Parent: ${browseData.parent_path}`}
              className="p-1 text-accent hover:bg-panel rounded transition-colors cursor-pointer shrink-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-highlight mr-1"
            >
              <ArrowUp className="w-4 h-4" />
            </button>
          )}

          <div className="flex items-center flex-wrap gap-1 min-w-0">
            {breadcrumbs.map((crumb, idx) => {
              const isLast = idx === breadcrumbs.length - 1;
              return (
                <React.Fragment key={crumb.path}>
                  <button
                    type="button"
                    onClick={() => handleNavigate(crumb.path)}
                    disabled={isLast}
                    className={`hover:underline cursor-pointer px-1 py-0.5 rounded transition-colors ${
                      isLast
                        ? 'text-accent font-bold cursor-default hover:no-underline'
                        : 'text-muted hover:text-text-main'
                    }`}
                  >
                    {crumb.name}
                  </button>
                  {!isLast && (
                    <ChevronRight className="w-3 h-3 text-muted/60 shrink-0" />
                  )}
                </React.Fragment>
              );
            })}
          </div>
        </div>

        {/* Error State */}
        {error && (
          <div className="mb-3 p-3 bg-cta/15 border border-cta/30 rounded-xl text-sm text-cta flex items-center justify-between gap-2">
            <div className="flex items-center gap-2 min-w-0">
              <AlertCircle className="w-4 h-4 shrink-0" />
              <span className="truncate">{error}</span>
            </div>
            <button
              type="button"
              onClick={() => fetchPath(currentPath)}
              className="px-2.5 py-1 text-xs bg-cta text-white font-medium rounded-lg hover:bg-cta-hover transition-colors cursor-pointer shrink-0"
            >
              Retry
            </button>
          </div>
        )}

        {/* Directory List Area */}
        <div className="flex-1 overflow-y-auto border border-border-subtle rounded-xl bg-canvas/40 p-2 min-h-52 max-h-72">
          {isLoading ? (
            <div className="h-48 flex flex-col items-center justify-center text-muted gap-2">
              <Loader2 className="w-6 h-6 animate-spin text-accent" />
              <span className="text-xs">Browsing filesystem...</span>
            </div>
          ) : !browseData || browseData.directories.length === 0 ? (
            <div className="h-48 flex flex-col items-center justify-center text-muted gap-2">
              <Folder className="w-8 h-8 text-muted/40" />
              <span className="text-xs">No subdirectories found</span>
            </div>
          ) : (
            <div className="space-y-1">
              {browseData.directories.map((dir) => (
                <button
                  key={dir.path}
                  type="button"
                  onClick={() => handleNavigate(dir.path)}
                  className="w-full flex items-center justify-between px-3 py-2 rounded-xl text-left text-sm text-text-main hover:bg-panel hover:text-accent border border-transparent hover:border-border-subtle transition-colors cursor-pointer group focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-highlight"
                >
                  <div className="flex items-center gap-2.5 min-w-0">
                    <Folder className="w-4 h-4 text-accent shrink-0 group-hover:scale-110 transition-transform" />
                    <span className="truncate font-mono text-xs">{dir.name}</span>
                  </div>
                  <ChevronRight className="w-3.5 h-3.5 text-muted group-hover:text-accent transition-colors shrink-0" />
                </button>
              ))}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="pt-4 mt-4 border-t border-border-subtle flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-3">
          <div className="flex-1 min-w-0">
            <div className="text-[10px] uppercase tracking-wider text-muted font-semibold mb-0.5">
              Selected Path
            </div>
            <div
              className="text-xs font-mono text-accent truncate bg-canvas/80 px-2.5 py-1.5 rounded-lg border border-border-subtle"
              title={currentPath}
            >
              {currentPath || '/'}
            </div>
          </div>
          <div className="flex items-center justify-end gap-2.5 shrink-0">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 bg-panel-hover hover:bg-canvas text-text-main border border-border-subtle rounded-xl text-sm font-medium transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
            >
              Cancel
            </button>
            <button
              type="button"
              disabled={isLoading || !currentPath}
              onClick={handleSelectCurrent}
              className="px-4 py-2 bg-cta hover:bg-cta-hover text-white font-semibold rounded-xl text-sm transition-colors shadow-md disabled:opacity-50 cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
            >
              Select This Folder
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
