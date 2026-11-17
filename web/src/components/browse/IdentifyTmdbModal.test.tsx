import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { IdentifyTmdbModal } from './IdentifyTmdbModal';
import { ItemDetailsModal } from './ItemDetailsModal';
import { api } from '../../api/client';
import type { ItemDetailsPayload } from '../../types';

describe('IdentifyTmdbModal & ItemDetailsModal Admin Actions', () => {
  const movieDetails: ItemDetailsPayload = {
    card: {
      id: 77,
      title: 'Matrix',
      media_type: 'movie',
      release_year: 1999,
    },
    overview: 'Raw scanner overview.',
    genres: [],
    stream_url: '/api/v1/stream/77',
  };

  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('hides Edit Metadata and Identify buttons for standard users and shows them for admins', () => {
    const { rerender } = render(
      <ItemDetailsModal
        itemId={77}
        initialDetails={movieDetails}
        currentUser={{ id: 'u-std', username: 'viewer', role: 'standard' }}
        onClose={vi.fn()}
      />
    );

    expect(screen.queryByRole('button', { name: /Edit Metadata/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Identify/i })).not.toBeInTheDocument();

    rerender(
      <ItemDetailsModal
        itemId={77}
        initialDetails={movieDetails}
        currentUser={{ id: 'u-adm', username: 'admin', role: 'admin' }}
        onClose={vi.fn()}
      />
    );

    expect(screen.getByRole('button', { name: /Edit Metadata/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Identify/i })).toBeInTheDocument();
  });

  it('searches TMDb candidates, previews poster/year/overview, and 1-click Applies refreshing ItemDetailsModal', async () => {
    const searchSpy = vi.spyOn(api, 'searchItemTmdb').mockResolvedValue({
      results: [
        {
          tmdb_id: 603,
          media_type: 'movie',
          title: 'The Matrix',
          release_year: 1999,
          rating: 8.7,
          overview: 'A computer hacker learns about the true nature of reality.',
          poster_url: 'https://image.tmdb.org/t/p/w500/matrix.jpg',
        },
      ],
    });

    const enrichedDetails: ItemDetailsPayload = {
      ...movieDetails,
      card: {
        ...movieDetails.card,
        title: 'The Matrix',
        rating: 8.7,
        poster_url: '/api/v1/artwork/77/poster',
      },
      overview: 'A computer hacker learns about the true nature of reality.',
      director: 'Lana Wachowski, Lilly Wachowski',
      actors: ['Keanu Reeves', 'Carrie-Anne Moss'],
      genres: ['Action', 'Science Fiction'],
      locked_fields: ['title', 'overview', 'director', 'actors', 'genres'],
    };

    const applySpy = vi.spyOn(api, 'applyItemTmdb').mockResolvedValue(enrichedDetails);

    render(
      <ItemDetailsModal
        itemId={77}
        initialDetails={movieDetails}
        currentUser={{ id: 'u-adm', username: 'admin', role: 'admin' }}
        onClose={vi.fn()}
      />
    );

    fireEvent.click(screen.getByRole('button', { name: /Identify/i }));

    await waitFor(() => {
      expect(searchSpy).toHaveBeenCalledWith(77, {
        q: 'Matrix',
        year: 1999,
        media_type: 'movie',
      });
      expect(screen.getByText('The Matrix')).toBeInTheDocument();
      expect(
        screen.getByText(/A computer hacker learns about the true nature of reality/i)
      ).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole('button', { name: /Apply The Matrix/i }));

    await waitFor(() => {
      expect(applySpy).toHaveBeenCalledWith(77, {
        tmdb_id: 603,
        media_type: 'movie',
        download_artwork: true,
      });
      // ItemDetailsModal reflects updated metadata immediately!
      expect(screen.getByText('Lana Wachowski, Lilly Wachowski')).toBeInTheDocument();
      expect(screen.getByText('Keanu Reeves, Carrie-Anne Moss')).toBeInTheDocument();
    });
  });

  it('allows switching media type and re-searching in IdentifyTmdbModal directly', async () => {
    const searchSpy = vi
      .spyOn(api, 'searchItemTmdb')
      .mockResolvedValue({ results: [] });

    render(
      <IdentifyTmdbModal
        isOpen={true}
        itemId={77}
        details={movieDetails}
        onClose={vi.fn()}
        onApplied={vi.fn()}
      />
    );

    await waitFor(() => expect(searchSpy).toHaveBeenCalledTimes(1));

    fireEvent.change(screen.getByLabelText('TMDb search query'), {
      target: { value: 'Breaking Bad' },
    });
    fireEvent.change(screen.getByLabelText('Media type'), {
      target: { value: 'tv' },
    });
    fireEvent.click(screen.getByRole('button', { name: /^Search$/i }));

    await waitFor(() => {
      expect(searchSpy).toHaveBeenLastCalledWith(77, {
        q: 'Breaking Bad',
        year: 1999,
        media_type: 'tv',
      });
    });
  });
});
