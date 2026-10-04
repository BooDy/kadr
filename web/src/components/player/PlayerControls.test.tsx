import { describe, it, expect, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { PlayerControls, type PlayerControlsProps } from './PlayerControls';
import type { SubtitleTrack, SubtitleSearchResult } from '../../types';

describe('PlayerControls Component', () => {
  const defaultProps: PlayerControlsProps = {
    title: 'Interstellar',
    subtitle: 'Christopher Nolan',
    releaseYear: 2014,
    isPlaying: true,
    currentTime: 120,
    duration: 7200,
    volume: 0.8,
    isMuted: false,
    isFullscreen: false,
    subtitles: [],
    activeSubtitleId: null,
    isVisible: true,
    onPlayPause: vi.fn(),
    onSeek: vi.fn(),
    onVolumeChange: vi.fn(),
    onToggleMute: vi.fn(),
    onToggleFullscreen: vi.fn(),
    onSelectSubtitle: vi.fn(),
    onClose: vi.fn(),
  };

  const sampleTracks: SubtitleTrack[] = [
    {
      id: 1,
      media_item_id: 10,
      source: 'sidecar',
      language: 'en',
      title: 'English CC',
      format: 'srt',
      is_default: true,
      is_forced: false,
      stream_url: '/api/v1/subtitles/1/stream.vtt',
    },
    {
      id: 2,
      media_item_id: 10,
      source: 'downloaded',
      language: 'es',
      title: 'Spanish',
      format: 'srt',
      is_default: false,
      is_forced: false,
      stream_url: '/api/v1/subtitles/2/stream.vtt',
    },
  ];

  it('renders search online button when no subtitles and switches to search view', async () => {
    const onSearch = vi.fn().mockResolvedValue([
      { id: 'sub-1', language: 'en', format: 'srt', release_name: 'Interstellar.1080p' },
    ]);
    render(<PlayerControls {...defaultProps} subtitles={[]} onSearchSubtitles={onSearch} />);
    fireEvent.click(screen.getByRole('button', { name: 'Subtitles' }));
    expect(screen.getByRole('button', { name: /Search OpenSubtitles Online/i })).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /Search OpenSubtitles Online/i }));
    expect(screen.getByPlaceholderText(/Language/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /Search/i }));
    expect(onSearch).toHaveBeenCalledWith('en');
    expect(await screen.findByText(/Interstellar.1080p/i)).toBeInTheDocument();
  });

  it('allows entering a custom language filter and searching', async () => {
    const onSearch = vi.fn().mockResolvedValue([
      { id: 'sub-fr-1', language: 'fr', format: 'srt', release_name: 'Interstellar.French' },
    ]);
    render(<PlayerControls {...defaultProps} subtitles={[]} onSearchSubtitles={onSearch} />);
    fireEvent.click(screen.getByRole('button', { name: 'Subtitles' }));
    fireEvent.click(screen.getByRole('button', { name: /Search OpenSubtitles Online/i }));

    const input = screen.getByPlaceholderText(/Language/i);
    fireEvent.change(input, { target: { value: 'fr' } });
    fireEvent.click(screen.getByRole('button', { name: /Search/i }));

    expect(onSearch).toHaveBeenCalledWith('fr');
    expect(await screen.findByText(/Interstellar.French/i)).toBeInTheDocument();
  });

  it('falls back to "en" when search input is empty or whitespace', async () => {
    const onSearch = vi.fn().mockResolvedValue([]);
    render(<PlayerControls {...defaultProps} subtitles={[]} onSearchSubtitles={onSearch} />);
    fireEvent.click(screen.getByRole('button', { name: 'Subtitles' }));
    fireEvent.click(screen.getByRole('button', { name: /Search OpenSubtitles Online/i }));

    const input = screen.getByPlaceholderText(/Language/i);
    fireEvent.change(input, { target: { value: '   ' } });
    fireEvent.click(screen.getByRole('button', { name: /Search/i }));

    expect(onSearch).toHaveBeenCalledWith('en');
  });

  it('clicking download on a match calls onDownloadSubtitle and shows loading state', async () => {
    const searchMatch: SubtitleSearchResult = {
      id: 'sub-1',
      language: 'en',
      format: 'srt',
      release_name: 'Interstellar.1080p',
    };
    const onSearch = vi.fn().mockResolvedValue([searchMatch]);
    let resolveDownload: () => void = () => {};
    const onDownload = vi.fn().mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          resolveDownload = resolve;
        })
    );

    render(
      <PlayerControls
        {...defaultProps}
        subtitles={[]}
        onSearchSubtitles={onSearch}
        onDownloadSubtitle={onDownload}
      />
    );
    fireEvent.click(screen.getByRole('button', { name: 'Subtitles' }));
    fireEvent.click(screen.getByRole('button', { name: /Search OpenSubtitles Online/i }));
    fireEvent.click(screen.getByRole('button', { name: /Search/i }));

    await screen.findByText(/Interstellar.1080p/i);
    const downloadBtn = screen.getByRole('button', { name: /Download/i });
    fireEvent.click(downloadBtn);

    expect(onDownload).toHaveBeenCalledWith(searchMatch);
    expect(screen.getByText(/Downloading/i)).toBeInTheDocument();

    resolveDownload();
    await waitFor(() => {
      expect(screen.queryByText(/Downloading/i)).not.toBeInTheDocument();
    });
  });

  it('displays prompt banner when recentlyDownloadedId is provided', () => {
    render(
      <PlayerControls
        {...defaultProps}
        subtitles={sampleTracks}
        recentlyDownloadedId={2}
      />
    );
    fireEvent.click(screen.getByRole('button', { name: 'Subtitles' }));
    expect(screen.getByText(/Subtitle downloaded! Click below to display on screen/i)).toBeInTheDocument();
  });

  it('renders "+ Search & Download Online" button when subtitles are present and switches to search view', () => {
    render(<PlayerControls {...defaultProps} subtitles={sampleTracks} />);
    fireEvent.click(screen.getByRole('button', { name: 'Subtitles' }));

    expect(screen.getByText('English CC')).toBeInTheDocument();
    expect(screen.getByText('Spanish')).toBeInTheDocument();
    const searchBtn = screen.getByRole('button', { name: /\+ Search & Download Online/i });
    expect(searchBtn).toBeInTheDocument();

    fireEvent.click(searchBtn);
    expect(screen.getByPlaceholderText(/Language/i)).toBeInTheDocument();
  });

  it('navigates back to track list when clicking back button in search view', () => {
    render(<PlayerControls {...defaultProps} subtitles={sampleTracks} />);
    fireEvent.click(screen.getByRole('button', { name: 'Subtitles' }));
    fireEvent.click(screen.getByRole('button', { name: /\+ Search & Download Online/i }));

    expect(screen.getByPlaceholderText(/Language/i)).toBeInTheDocument();
    const backBtn = screen.getByRole('button', { name: /Back to Subtitles/i });
    fireEvent.click(backBtn);

    expect(screen.getByText('English CC')).toBeInTheDocument();
  });

  it('notifies parent via onSubtitlesMenuToggle when menu opens and closes', () => {
    const onToggle = vi.fn();
    render(<PlayerControls {...defaultProps} onSubtitlesMenuToggle={onToggle} />);

    fireEvent.click(screen.getByRole('button', { name: 'Subtitles' }));
    expect(onToggle).toHaveBeenCalledWith(true);

    fireEvent.click(screen.getByRole('button', { name: 'Subtitles' }));
    expect(onToggle).toHaveBeenCalledWith(false);
  });

  it('displays search error when onSearchSubtitles fails', async () => {
    const onSearch = vi.fn().mockRejectedValue(new Error('OpenSubtitles unconfigured'));
    render(<PlayerControls {...defaultProps} subtitles={[]} onSearchSubtitles={onSearch} />);
    fireEvent.click(screen.getByRole('button', { name: 'Subtitles' }));
    fireEvent.click(screen.getByRole('button', { name: /Search OpenSubtitles Online/i }));
    fireEvent.click(screen.getByRole('button', { name: /Search/i }));

    expect(await screen.findByText(/Failed to search online subtitles/i)).toBeInTheDocument();
  });

  it('displays download error when onDownloadSubtitle fails', async () => {
    const searchMatch: SubtitleSearchResult = {
      id: 'sub-fail',
      language: 'en',
      format: 'srt',
      release_name: 'Failed.Release',
    };
    const onSearch = vi.fn().mockResolvedValue([searchMatch]);
    const onDownload = vi.fn().mockRejectedValue(new Error('Network error'));

    render(
      <PlayerControls
        {...defaultProps}
        subtitles={[]}
        onSearchSubtitles={onSearch}
        onDownloadSubtitle={onDownload}
      />
    );
    fireEvent.click(screen.getByRole('button', { name: 'Subtitles' }));
    fireEvent.click(screen.getByRole('button', { name: /Search OpenSubtitles Online/i }));
    fireEvent.click(screen.getByRole('button', { name: /Search/i }));

    await screen.findByText(/Failed.Release/i);
    fireEvent.click(screen.getByRole('button', { name: /Download/i }));

    expect(await screen.findByText(/Failed to download subtitle/i)).toBeInTheDocument();
  });
});
