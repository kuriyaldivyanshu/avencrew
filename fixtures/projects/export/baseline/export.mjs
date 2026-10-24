export function total(rows) {
  return rows.slice(0, 50000).reduce((sum, row) => sum + row.cents, 0);
}
