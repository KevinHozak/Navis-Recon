import test from 'node:test';
import assert from 'node:assert/strict';
import { canSelectCategory, selectableCategories } from './categorySelectorState.js';

test('category selection is confined to eligible options and disabled while busy', () => {
  const permitted = ['Expenses:Food', 'Income:Refund'];
  assert.deepEqual(selectableCategories(permitted, 'food'), ['Expenses:Food']);
  assert.equal(canSelectCategory('Expenses:Food', permitted, false), true);
  assert.equal(canSelectCategory('Expenses:Forged', permitted, false), false);
  assert.equal(canSelectCategory('Expenses:Food', permitted, true), false);
  assert.equal(selectableCategories([], '').length, 0);
});
