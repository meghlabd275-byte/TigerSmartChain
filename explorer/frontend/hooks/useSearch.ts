// useSearch.ts - Hook for explorer search functionality
import { useState, useCallback } from 'react';
import { useExplorer } from './useExplorer';

interface SearchResult {
  type: 'address' | 'block' | 'transaction' | 'token' | 'validator';
  data: any;
}

interface UseSearchResult {
  searchResults: SearchResult[];
  searching: boolean;
  error: string | null;
  search: (query: string) => Promise<void>;
  clearResults: () => void;
}

export const useSearch = (): UseSearchResult => {
  const { searchAPI } = useExplorer();
  const [searchResults, setSearchResults] = useState<SearchResult[]>([]);
  const [searching, setSearching] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const search = useCallback(async (query: string) => {
    if (!query || query.trim().length === 0) {
      setSearchResults([]);
      return;
    }

    setSearching(true);
    setError(null);

    try {
      const results = await searchAPI(query);
      setSearchResults(results);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Search failed');
      setSearchResults([]);
    } finally {
      setSearching(false);
    }
  }, [searchAPI]);

  const clearResults = useCallback(() => {
    setSearchResults([]);
    setError(null);
  }, []);

  return {
    searchResults,
    searching,
    error,
    search,
    clearResults,
  };
};

export default useSearch;