import test from 'node:test';
import assert from 'node:assert/strict';
import { fingerprintSdkSource } from '../check-sdk-api.mjs';

test('SDK declaration guard detects enum payload, public field and reexport changes', () => {
  for (const [before, after] of [
    ['pub enum Result { Ready, Failed(String) }', 'pub enum Result { Ready, Failed(u64) }'],
    ['pub struct Page { pub count: u64 }', 'pub struct Page { pub count: String }'],
    ['pub use types::{A, B};', 'pub use types::{A, C};'],
    ['#[serde(rename_all = "camelCase")] pub struct Dto {}', '#[serde(rename_all = "snake_case")] pub struct Dto {}'],
  ]) assert.notDeepEqual(fingerprintSdkSource(before), fingerprintSdkSource(after));
});
test('SDK guard excludes function implementation edits and handles braces in string literals', () => {
  const before = 'pub fn compute() -> u64 { 1 } pub enum Kind { #[serde(rename="{")] Value(String) }';
  const after = before.replace('{ 1 }', '{ 2 }');
  assert.deepEqual(fingerprintSdkSource(before), fingerprintSdkSource(after));
  assert.ok(fingerprintSdkSource(before).publicItems.some((value) => value.includes('Value(String)')));
});

test('SDK guard rejects method ownership/removal, trait impl, paths and bounds regressions', () => {
  for (const [before, after] of [
    ['pub struct A; pub struct B; impl A { pub fn new() -> Self { Self } }', 'pub struct A; pub struct B; impl B { pub fn new() -> Self { Self } }'],
    ['impl A { pub fn new() -> Self { Self } }', 'impl A {}'],
    ['impl Display for A { fn fmt(&self, f: &mut F) -> Result { todo!() } }', ''],
    ['pub mod first { pub struct A; }', 'pub mod second { pub struct A; }'],
    ['pub mod first { pub struct A; }', 'mod first { pub struct A; }'],
    ['mod first; pub use first::A;', 'mod first; pub use first::A as B;'],
    ['impl<T: Clone> A<T> { pub fn new(t: T) -> Self { todo!() } }', 'impl<T: Copy> A<T> { pub fn new(t: T) -> Self { todo!() } }'],
    ['impl<T> A<T> where T: Clone { pub fn new(t: T) -> Self { todo!() } }', 'impl<T> A<T> where T: Copy { pub fn new(t: T) -> Self { todo!() } }'],
    ['pub trait Api { fn get<T: Clone>(&self) -> T; }', 'pub trait Api { fn get<T: Copy>(&self) -> T; }'],
    ['impl Iterator for A { type Item = String; fn next(&mut self) -> Option<Self::Item> { None } }', 'impl Iterator for A { type Item = u64; fn next(&mut self) -> Option<Self::Item> { None } }'],
  ]) assert.notDeepEqual(fingerprintSdkSource(before), fingerprintSdkSource(after), before);
});
test('SDK guard ignores inherent, trait default and trait implementation method bodies', () => {
  const before = 'pub trait Api { fn get(&self) -> u64 { 1 } } impl Api for A { fn get(&self) -> u64 { 2 } } impl A { pub fn new() -> Self { Self } fn internal() { 7 } }';
  const after = before.replace('{ 1 }', '{ 3 }').replace('{ 2 }', '{ 4 }').replace('{ Self }', '{ todo!() }').replace('{ 7 }', '{ 8 }');
  assert.deepEqual(fingerprintSdkSource(before), fingerprintSdkSource(after));
});
