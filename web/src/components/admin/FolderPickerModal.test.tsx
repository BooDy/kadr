import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { FolderPickerModal } from './FolderPickerModal';
import { api } from '../../api/client';

describe('FolderPickerModal', () => {
  const mockBrowseResponse = {
    current_path: '/media',
    parent_path: '/',
    directories: [
      { name: 'movies', path: '/media/movies' },
      { name: 'shows', path: '/media/shows' },
    ],
    shortcuts: [
      { name: 'Root (/)', path: '/' },
      { name: 'Media', path: '/media' },
      { name: 'Home', path: '/home/user' },
    ],
  };

  beforeEach(() => {
    vi.clearAllMocks();
    vi.spyOn(api, 'browseFilesystem').mockResolvedValue(mockBrowseResponse);
  });

  it('renders breadcrumbs, shortcuts, and directory list', async () => {
    render(<FolderPickerModal isOpen={true} onClose={vi.fn()} onSelect={vi.fn()} />);

    expect(screen.getByText('Select Server Folder')).toBeInTheDocument();

    await waitFor(() => {
      expect(screen.getByText('movies')).toBeInTheDocument();
      expect(screen.getByText('shows')).toBeInTheDocument();
      expect(screen.getByText('Root (/)')).toBeInTheDocument();
      expect(screen.getByText('Media')).toBeInTheDocument();
      expect(screen.getByText('Home')).toBeInTheDocument();
    });
  });

  it('calls onSelect with selected directory path when CTA clicked', async () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();
    render(<FolderPickerModal isOpen={true} onClose={onClose} onSelect={onSelect} />);

    await waitFor(() => expect(screen.getByText('movies')).toBeInTheDocument());

    const selectBtn = screen.getByRole('button', { name: /Select This Folder/i });
    fireEvent.click(selectBtn);

    expect(onSelect).toHaveBeenCalledWith('/media');
    expect(onClose).toHaveBeenCalled();
  });

  it('navigates into directory when folder is clicked', async () => {
    vi.spyOn(api, 'browseFilesystem').mockImplementation(async (path) => {
      if (path === '/media/movies') {
        return {
          current_path: '/media/movies',
          parent_path: '/media',
          directories: [
            { name: 'action', path: '/media/movies/action' },
            { name: 'sci-fi', path: '/media/movies/sci-fi' },
          ],
          shortcuts: mockBrowseResponse.shortcuts,
        };
      }
      return mockBrowseResponse;
    });

    render(<FolderPickerModal isOpen={true} onClose={vi.fn()} onSelect={vi.fn()} />);

    await waitFor(() => expect(screen.getByText('movies')).toBeInTheDocument());

    fireEvent.click(screen.getByText('movies'));

    await waitFor(() => {
      expect(api.browseFilesystem).toHaveBeenCalledWith('/media/movies');
      expect(screen.getByText('action')).toBeInTheDocument();
      expect(screen.getByText('sci-fi')).toBeInTheDocument();
    });
  });

  it('navigates to shortcut path when shortcut pill is clicked', async () => {
    render(<FolderPickerModal isOpen={true} onClose={vi.fn()} onSelect={vi.fn()} />);

    await waitFor(() => expect(screen.getByText('Root (/)')).toBeInTheDocument());

    fireEvent.click(screen.getByText('Root (/)'));

    await waitFor(() => {
      expect(api.browseFilesystem).toHaveBeenCalledWith('/');
    });
  });

  it('navigates to parent path when up button is clicked', async () => {
    render(<FolderPickerModal isOpen={true} onClose={vi.fn()} onSelect={vi.fn()} />);

    await waitFor(() => expect(screen.getByRole('button', { name: /Up one folder|Go to parent folder/i })).toBeInTheDocument());

    const upBtn = screen.getByRole('button', { name: /Up one folder|Go to parent folder/i });
    fireEvent.click(upBtn);

    await waitFor(() => {
      expect(api.browseFilesystem).toHaveBeenCalledWith('/');
    });
  });

  it('navigates when breadcrumb segment is clicked', async () => {
    vi.spyOn(api, 'browseFilesystem').mockResolvedValue({
      current_path: '/media/movies/action',
      parent_path: '/media/movies',
      directories: [],
      shortcuts: mockBrowseResponse.shortcuts,
    });

    render(<FolderPickerModal isOpen={true} onClose={vi.fn()} onSelect={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'media' })).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole('button', { name: 'media' }));

    await waitFor(() => {
      expect(api.browseFilesystem).toHaveBeenCalledWith('/media');
    });
  });

  it('calls onClose when close icon or Cancel button is clicked', async () => {
    const onClose = vi.fn();
    render(<FolderPickerModal isOpen={true} onClose={onClose} onSelect={vi.fn()} />);

    await waitFor(() => expect(screen.getByText('movies')).toBeInTheDocument());

    const cancelBtn = screen.getByRole('button', { name: /Cancel/i });
    fireEvent.click(cancelBtn);
    expect(onClose).toHaveBeenCalledTimes(1);

    const closeBtn = screen.getByRole('button', { name: /Close folder picker/i });
    fireEvent.click(closeBtn);
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it('calls onClose when Escape key is pressed', async () => {
    const onClose = vi.fn();
    render(<FolderPickerModal isOpen={true} onClose={onClose} onSelect={vi.fn()} />);

    await waitFor(() => expect(screen.getByText('movies')).toBeInTheDocument());

    fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('displays error message and allows retrying if browse fails', async () => {
    vi.spyOn(api, 'browseFilesystem').mockRejectedValueOnce(new Error('Permission denied'));

    render(<FolderPickerModal isOpen={true} onClose={vi.fn()} onSelect={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText('Permission denied')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /Retry/i })).toBeInTheDocument();
    });

    vi.spyOn(api, 'browseFilesystem').mockResolvedValueOnce(mockBrowseResponse);
    fireEvent.click(screen.getByRole('button', { name: /Retry/i }));

    await waitFor(() => {
      expect(screen.getByText('movies')).toBeInTheDocument();
    });
  });

  it('does not render when isOpen is false', () => {
    const { container } = render(<FolderPickerModal isOpen={false} onClose={vi.fn()} onSelect={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });
});
