import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { MetadataEditorModal } from './MetadataEditorModal';
import { api } from '../../api/client';
import type { ItemDetailsPayload } from '../../types';

describe('MetadataEditorModal', () => {
  const baseDetails: ItemDetailsPayload = {
    card: {
      id: 42,
      title: 'Dune',
      media_type: 'movie',
      release_year: 2021,
      rating: 8.0,
    },
    original_title: 'Dune: Part One',
    overview: 'A noble family becomes embroiled in a war for Arrakis.',
    genres: ['Sci-Fi', 'Adventure'],
    director: 'Denis Villeneuve',
    actors: ['Timothée Chalamet', 'Zendaya'],
    studio: 'Legendary Pictures',
    country: 'US',
    locked_fields: ['title', 'director'],
    stream_url: '/api/v1/stream/42',
  };

  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('renders pre-populated fields and locked-field indicators', () => {
    render(
      <MetadataEditorModal
        isOpen={true}
        itemId={42}
        details={baseDetails}
        onClose={vi.fn()}
        onSaved={vi.fn()}
      />
    );

    expect(screen.getByLabelText('Title')).toHaveValue('Dune');
    expect(screen.getByLabelText('Original Title')).toHaveValue('Dune: Part One');
    expect(screen.getByLabelText('Year')).toHaveValue(2021);
    expect(screen.getByLabelText('Director')).toHaveValue('Denis Villeneuve');
    expect(screen.getByLabelText('Cast')).toHaveValue('Timothée Chalamet, Zendaya');
    expect(screen.getByTestId('locked-badge-title')).toBeInTheDocument();
    expect(screen.getByTestId('locked-badge-director')).toBeInTheDocument();
    expect(screen.queryByTestId('locked-badge-studio')).not.toBeInTheDocument();
    expect(screen.getByText(/2 fields locked against rescan overwrites/i)).toBeInTheDocument();
  });

  it('submits only modified fields and remote artwork URLs via PATCH', async () => {
    const updatedDetails: ItemDetailsPayload = {
      ...baseDetails,
      card: { ...baseDetails.card, title: 'Dune (Director Cut)', rating: 8.6 },
      locked_fields: ['title', 'director', 'rating', 'poster_path'],
    };
    const updateSpy = vi
      .spyOn(api, 'updateItemMetadata')
      .mockResolvedValueOnce(updatedDetails);
    const onSaved = vi.fn();
    const onClose = vi.fn();

    render(
      <MetadataEditorModal
        isOpen={true}
        itemId={42}
        details={baseDetails}
        onClose={onClose}
        onSaved={onSaved}
      />
    );

    fireEvent.change(screen.getByLabelText('Title'), {
      target: { value: 'Dune (Director Cut)' },
    });
    fireEvent.change(screen.getByLabelText('Rating'), {
      target: { value: '8.6' },
    });
    fireEvent.change(screen.getByLabelText('Poster URL'), {
      target: { value: 'https://image.tmdb.org/t/p/w500/dune.jpg' },
    });

    fireEvent.click(screen.getByRole('button', { name: /Save Changes/i }));

    await waitFor(() => {
      expect(updateSpy).toHaveBeenCalledWith(42, {
        title: 'Dune (Director Cut)',
        rating: 8.6,
        poster_url: 'https://image.tmdb.org/t/p/w500/dune.jpg',
      });
      expect(onSaved).toHaveBeenCalledWith(updatedDetails);
      expect(onClose).toHaveBeenCalled();
    });
  });

  it('calls updateItemMetadata with reset_locks: true when Reset Locks is clicked', async () => {
    const unlockedDetails: ItemDetailsPayload = {
      ...baseDetails,
      locked_fields: [],
    };
    const updateSpy = vi
      .spyOn(api, 'updateItemMetadata')
      .mockResolvedValueOnce(unlockedDetails);
    const onSaved = vi.fn();

    render(
      <MetadataEditorModal
        isOpen={true}
        itemId={42}
        details={baseDetails}
        onClose={vi.fn()}
        onSaved={onSaved}
      />
    );

    fireEvent.click(screen.getByRole('button', { name: /Reset Locks/i }));

    await waitFor(() => {
      expect(updateSpy).toHaveBeenCalledWith(42, { reset_locks: true });
      expect(onSaved).toHaveBeenCalledWith(unlockedDetails);
    });
  });
});
