export {};

declare global {
    type PagefindIndexOptions = {
        basePath?: string,
        baseUrl?: string,
        excerptLength?: number,
        primary?: boolean,
        indexWeight?: number,
        mergeFilter?: Object,
        language?: string,
    };

    type PagefindSearchOptions = {
        preload?: boolean,
        verbose?: boolean,
        filters?: Object,
        sort?: Object,
    }

    type PagefindFilterCounts = Record<string, Record<string, number>>;

    type PagefindSearchResults = {
        results: PagefindSearchResult[],
        unfilteredResultCount: number,
        filters: PagefindFilterCounts,
        totalFilters: PagefindFilterCounts,
        timings: {
            preload: number,
            search: number,
            total: number
        }
    }

    type PagefindSearchResult = {
        id: string,
        score: number,
        words: number[],
        data: () => Promise<PagefindSearchFragment>
    }

    type PagefindSearchFragment = {
        url: string,
        raw_url?: string
        content: string,
        raw_content?: string;
        excerpt: string,
        sub_results: PagefindSubResult[],
        word_count: number,
        locations: number[],
        weighted_locations: PagefindWordLocation[],
        filters: Record<string, string[]>
        meta: Record<string, string>,
        anchors: PagefindSearchAnchor[],
    }

    type PagefindSubResult = {
        title: string,
        url: string,
        locations: number[],
        weighted_locations: PagefindWordLocation[],
        excerpt: string,
        anchor?: PagefindSearchAnchor,
    }

    type PagefindWordLocation = {
        weight: number,
        location: number,
    }

    type PagefindSearchAnchor = {
        element: string,
        id: string,
        text?: string,
        location: number,
    }
}
