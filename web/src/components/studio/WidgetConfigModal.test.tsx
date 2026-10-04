import { describe, it, expect, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { WidgetConfigModal } from './WidgetConfigModal';
import type { Library, WidgetNode } from '../../types';

describe('WidgetConfigModal Component', () => {
  const mockLibraries: Library[] = [
    {
      id: 'lib-1',
      name: 'Movies',
      path: '/media/movies',
      media_type: 'movie',
      is_private: false,
      created_at: 1700000000,
    },
    {
      id: 'lib-2',
      name: 'TV Shows',
      path: '/media/shows',
      media_type: 'show',
      is_private: false,
      created_at: 1700000000,
    },
    {
      id: 'lib-anime',
      name: 'Anime',
      path: '/media/anime',
      media_type: 'anime',
      is_private: false,
      created_at: 1700000000,
    },
  ];

  it('renders with default fields when creating a new widget', () => {
    render(
      <WidgetConfigModal
        isOpen={true}
        libraries={mockLibraries}
        onSave={vi.fn()}
        onClose={vi.fn()}
      />
    );

    // Modal dialog accessibility
    const dialog = screen.getByRole('dialog');
    expect(dialog).toBeInTheDocument();
    expect(dialog).toHaveAttribute('aria-modal', 'true');
    expect(screen.getByText(/Add Widget/i)).toBeInTheDocument();

    // Default widget type: carousel
    const typeSelect = screen.getByLabelText(/Widget Type/i) as HTMLSelectElement;
    expect(typeSelect.value).toBe('carousel');

    // Title input rendered for carousel with empty default
    const titleInput = screen.getByLabelText(/Title/i) as HTMLInputElement;
    expect(titleInput.value).toBe('');

    // Query macro select default: recently_added
    const macroSelect = screen.getByLabelText(/Query Macro/i) as HTMLSelectElement;
    expect(macroSelect.value).toBe('recently_added');

    // Limit input default: 20
    const limitInput = screen.getByLabelText(/Limit/i) as HTMLInputElement;
    expect(limitInput.value).toBe('20');

    // Columns input is NOT rendered for carousel
    expect(screen.queryByLabelText(/Columns/i)).not.toBeInTheDocument();
  });

  it('changes widget type between Hero, Carousel, and Grid (showing columns input only for Grid)', () => {
    render(
      <WidgetConfigModal
        isOpen={true}
        libraries={mockLibraries}
        onSave={vi.fn()}
        onClose={vi.fn()}
      />
    );

    const typeSelect = screen.getByLabelText(/Widget Type/i) as HTMLSelectElement;

    // Switch to Grid -> columns input should appear
    fireEvent.change(typeSelect, { target: { value: 'grid' } });
    expect(screen.getByLabelText(/Columns/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Title/i)).toBeInTheDocument();

    // Switch to Hero Banner -> columns input should disappear, and title input is hidden
    fireEvent.change(typeSelect, { target: { value: 'hero_banner' } });
    expect(screen.queryByLabelText(/Columns/i)).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/Title/i)).not.toBeInTheDocument();

    // Switch back to Carousel -> title input reappears, columns stays hidden
    fireEvent.change(typeSelect, { target: { value: 'carousel' } });
    expect(screen.queryByLabelText(/Columns/i)).not.toBeInTheDocument();
    expect(screen.getByLabelText(/Title/i)).toBeInTheDocument();
  });

  it('selects library_items macro and verifies library dropdown renders with passed libraries', () => {
    render(
      <WidgetConfigModal
        isOpen={true}
        libraries={mockLibraries}
        onSave={vi.fn()}
        onClose={vi.fn()}
      />
    );

    // Initially library dropdown is not visible
    expect(screen.queryByLabelText(/Target Library/i)).not.toBeInTheDocument();

    // Change macro to library_items
    const macroSelect = screen.getByLabelText(/Query Macro/i);
    fireEvent.change(macroSelect, { target: { value: 'library_items' } });

    // Target Library dropdown should appear
    const librarySelect = screen.getByLabelText(/Target Library/i) as HTMLSelectElement;
    expect(librarySelect).toBeInTheDocument();

    // Verify all mock libraries are rendered as options
    expect(screen.getByRole('option', { name: 'Movies' })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: 'TV Shows' })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: 'Anime' })).toBeInTheDocument();
  });

  it('submitting form calls onSave with well-formed WidgetNode for Carousel', () => {
    const handleSave = vi.fn();
    render(
      <WidgetConfigModal
        isOpen={true}
        libraries={mockLibraries}
        onSave={handleSave}
        onClose={vi.fn()}
      />
    );

    // Set title
    const titleInput = screen.getByLabelText(/Title/i);
    fireEvent.change(titleInput, { target: { value: 'Trending Action' } });

    // Set query macro to genre_shelf and provide genre
    const macroSelect = screen.getByLabelText(/Query Macro/i);
    fireEvent.change(macroSelect, { target: { value: 'genre_shelf' } });
    const genreInput = screen.getByLabelText(/Genre/i);
    fireEvent.change(genreInput, { target: { value: 'Action' } });

    // Set limit and sort
    const limitInput = screen.getByLabelText(/Limit/i);
    fireEvent.change(limitInput, { target: { value: '15' } });
    const sortSelect = screen.getByLabelText(/Sort/i);
    fireEvent.change(sortSelect, { target: { value: 'rating:desc' } });

    // Save
    fireEvent.click(screen.getByRole('button', { name: /Save Widget/i }));

    expect(handleSave).toHaveBeenCalledTimes(1);
    const saved = handleSave.mock.calls[0][0] as WidgetNode;
    expect(saved.type).toBe('carousel');
    if (saved.type === 'carousel') {
      expect(saved.title).toBe('Trending Action');
      expect(saved.id).toBeDefined();
      expect(saved.binding.macro_type).toEqual({ genre_shelf: { genre: 'Action' } });
      expect(saved.binding.limit).toBe(15);
      expect(saved.binding.sort).toBe('rating:desc');
    }
  });

  it('submitting form calls onSave with well-formed WidgetNode for Grid with library_items', () => {
    const handleSave = vi.fn();
    render(
      <WidgetConfigModal
        isOpen={true}
        libraries={mockLibraries}
        onSave={handleSave}
        onClose={vi.fn()}
      />
    );

    // Switch to Grid
    const typeSelect = screen.getByLabelText(/Widget Type/i);
    fireEvent.change(typeSelect, { target: { value: 'grid' } });

    // Set title and columns
    fireEvent.change(screen.getByLabelText(/Title/i), { target: { value: 'Anime Library' } });
    fireEvent.change(screen.getByLabelText(/Columns/i), { target: { value: '5' } });

    // Set macro to library_items and select Anime library
    fireEvent.change(screen.getByLabelText(/Query Macro/i), { target: { value: 'library_items' } });
    fireEvent.change(screen.getByLabelText(/Target Library/i), { target: { value: 'lib-anime' } });

    // Set limit and sort
    fireEvent.change(screen.getByLabelText(/Limit/i), { target: { value: '30' } });
    fireEvent.change(screen.getByLabelText(/Sort/i), { target: { value: 'title:asc' } });

    // Submit
    fireEvent.click(screen.getByRole('button', { name: /Save Widget/i }));

    expect(handleSave).toHaveBeenCalledTimes(1);
    const saved = handleSave.mock.calls[0][0] as WidgetNode;
    expect(saved.type).toBe('grid');
    if (saved.type === 'grid') {
      expect(saved.title).toBe('Anime Library');
      expect(saved.columns).toBe(5);
      expect(saved.binding.macro_type).toEqual({ library_items: { library_id: 'lib-anime' } });
      expect(saved.binding.limit).toBe(30);
      expect(saved.binding.sort).toBe('title:asc');
    }
  });

  it('submitting form calls onSave with well-formed WidgetNode for Hero Banner', () => {
    const handleSave = vi.fn();
    render(
      <WidgetConfigModal
        isOpen={true}
        libraries={mockLibraries}
        onSave={handleSave}
        onClose={vi.fn()}
      />
    );

    // Switch to Hero Banner
    const typeSelect = screen.getByLabelText(/Widget Type/i);
    fireEvent.change(typeSelect, { target: { value: 'hero_banner' } });

    // Set macro to spotlight_item and item id
    fireEvent.change(screen.getByLabelText(/Query Macro/i), { target: { value: 'spotlight_item' } });
    const itemIdInput = screen.getByLabelText(/Item ID/i);
    fireEvent.change(itemIdInput, { target: { value: '42' } });

    // Submit
    fireEvent.click(screen.getByRole('button', { name: /Save Widget/i }));

    expect(handleSave).toHaveBeenCalledTimes(1);
    const saved = handleSave.mock.calls[0][0] as WidgetNode;
    expect(saved.type).toBe('hero_banner');
    if (saved.type === 'hero_banner') {
      expect('title' in saved).toBe(false);
      expect(saved.binding.macro_type).toEqual({ spotlight_item: { item_id: 42 } });
    }
  });

  it('prefills fields when initialWidget is passed and preserves id', () => {
    const initialWidget: WidgetNode = {
      type: 'grid',
      id: 'existing-widget-123',
      title: 'Top TV Series',
      columns: 6,
      binding: {
        macro_type: { library_items: { library_id: 'lib-2' } },
        limit: 25,
        sort: 'rating:desc',
      },
    };

    const handleSave = vi.fn();
    render(
      <WidgetConfigModal
        isOpen={true}
        initialWidget={initialWidget}
        libraries={mockLibraries}
        onSave={handleSave}
        onClose={vi.fn()}
      />
    );

    expect(screen.getByText(/Edit Widget/i)).toBeInTheDocument();
    expect((screen.getByLabelText(/Widget Type/i) as HTMLSelectElement).value).toBe('grid');
    expect((screen.getByLabelText(/Title/i) as HTMLInputElement).value).toBe('Top TV Series');
    expect((screen.getByLabelText(/Columns/i) as HTMLInputElement).value).toBe('6');
    expect((screen.getByLabelText(/Query Macro/i) as HTMLSelectElement).value).toBe('library_items');
    expect((screen.getByLabelText(/Target Library/i) as HTMLSelectElement).value).toBe('lib-2');
    expect((screen.getByLabelText(/Limit/i) as HTMLInputElement).value).toBe('25');
    expect((screen.getByLabelText(/Sort/i) as HTMLSelectElement).value).toBe('rating:desc');

    // Save without edits
    fireEvent.click(screen.getByRole('button', { name: /Save Widget/i }));

    expect(handleSave).toHaveBeenCalledTimes(1);
    const saved = handleSave.mock.calls[0][0] as WidgetNode;
    expect(saved.id).toBe('existing-widget-123');
    expect(saved.type).toBe('grid');
  });

  it('escape key and cancel button call onClose', () => {
    const handleClose = vi.fn();
    const { rerender } = render(
      <WidgetConfigModal
        isOpen={true}
        libraries={mockLibraries}
        onSave={vi.fn()}
        onClose={handleClose}
      />
    );

    // Cancel button click
    const cancelButton = screen.getByRole('button', { name: /Cancel/i });
    fireEvent.click(cancelButton);
    expect(handleClose).toHaveBeenCalledTimes(1);

    // Close button (X) click
    const xButton = screen.getByLabelText(/Close modal/i);
    fireEvent.click(xButton);
    expect(handleClose).toHaveBeenCalledTimes(2);

    // Escape key press
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(handleClose).toHaveBeenCalledTimes(3);

    // When isOpen is false, nothing is rendered
    rerender(
      <WidgetConfigModal
        isOpen={false}
        libraries={mockLibraries}
        onSave={vi.fn()}
        onClose={handleClose}
      />
    );
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });
});
