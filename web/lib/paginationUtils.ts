export interface PaginationResult<T> {
  items: T[];
  page: number;
  perPage: number;
  total: number;
  totalPages: number;
}

export function paginate<T>(
  items: T[],
  page: number,
  perPage: number,
): PaginationResult<T> {
  if (perPage <= 0) {
    throw new RangeError('pageSize must be positive');
  }
  const total = items.length;
  const totalPages = Math.max(1, Math.ceil(total / perPage));
  const clampedPage = Math.max(1, Math.min(page, totalPages));
  const start = (clampedPage - 1) * perPage;
  const end = start + perPage;

  return {
    items: items.slice(start, end),
    page: clampedPage,
    perPage,
    total,
    totalPages,
  };
}
