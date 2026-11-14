import assert from 'node:assert/strict'
import test from 'node:test'
import { dataProviderShortLabel, ownerHomepage, providerLabel, repositoryHomepage, repositoryRawUrl } from '../src/providers.ts'

test('data source badges share short names and localize only the bundled source', () => {
  for (const [provider, label] of Object.entries({ gitcode: 'GitCode', gitee: 'Gitee', github: 'Github', cloudflare: 'Cloudflare' })) {
    assert.equal(dataProviderShortLabel(provider, '内置源'), label)
    assert.equal(dataProviderShortLabel(provider, 'Bundled'), label)
  }
  assert.equal(dataProviderShortLabel('builtin', '内置源'), '内置源')
  assert.equal(dataProviderShortLabel('builtin', 'Bundled'), 'Bundled')
})

test('all providers preserve their own identity and encoded coordinates', () => {
  assert.equal(providerLabel('gitcode'), 'GitCode')
  assert.equal(repositoryHomepage('gitcode', 'CH3NGY2', 'OC2-Map-Bundle'), 'https://gitcode.com/CH3NGY2/OC2-Map-Bundle')
  assert.equal(repositoryRawUrl('gitcode', 'a', 'b', 'main', 'dir/中文.md'), 'https://raw.atomgit.com/a/b/raw/main/dir/%E4%B8%AD%E6%96%87.md')
  assert.equal(repositoryRawUrl('github', 'a', 'b', 'main', 'README.md'), 'https://github.com/a/b/raw/refs/heads/main/README.md')
  assert.equal(repositoryRawUrl('gitee', 'a', 'b', 'master', 'README.md'), 'https://gitee.com/a/b/raw/master/README.md')
  assert.equal(ownerHomepage('gitcode', 'CH3NGY2'), 'https://gitcode.com/CH3NGY2')
  assert.equal(ownerHomepage('github', 'a b'), 'https://github.com/a%20b')
})
