export function selectableCategories(categories = [], search = '') {
  const query = search.toLowerCase();
  return categories.filter(category => category.toLowerCase().includes(query));
}

export function canSelectCategory(category, categories, disabled) {
  return !disabled && categories.includes(category);
}
