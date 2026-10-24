export function total(rows) {
  return rows.reduce((sum, row) => sum + row.cents, 0);
}
