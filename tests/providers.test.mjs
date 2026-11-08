import assert from 'node:assert/strict'
import test from 'node:test'
import { ownerHomepage, providerLabel, repositoryHomepage, repositoryRawUrl, sourceRepository } from '../src/providers.ts'

test('all providers preserve their own identity and encoded coordinates', () => {
  assert.equal(providerLabel('gitcode'), 'GitCode')
  assert.equal(repositoryHomepage('gitcode', 'CH3NGY2', 'OC2-Map-Bundle'), 'https://gitcode.com/CH3NGY2/OC2-Map-Bundle')
  assert.equal(repositoryRawUrl('gitcode', 'a', 'b', 'main', 'dir/中文.md'), 'https://raw.atomgit.com/a/b/raw/main/dir/%E4%B8%AD%E6%96%87.md')
  assert.equal(repositoryRawUrl('github', 'a', 'b', 'main', 'README.md'), 'https://github.com/a/b/raw/refs/heads/main/README.md')
  assert.equal(repositoryRawUrl('gitee', 'a', 'b', 'master', 'README.md'), 'https://gitee.com/a/b/raw/master/README.md')
})

test('source titles identify source documents and never choose an arbitrary listed repository', () => {
  const map = { provider: 'gitcode', username: 'CH3NGY2', repository: 'OC2-Map-Bundle' }
  assert.deepEqual(sourceRepository([map, { ...map, username: 'ch3ngy2' }]), map)
  assert.equal(sourceRepository([map, { ...map, repository: 'other' }]), null)
  assert.equal(sourceRepository([]), null)
  assert.equal(ownerHomepage('gitcode', 'CH3NGY2'), 'https://gitcode.com/CH3NGY2')
  assert.deepEqual(sourceRepository([map], 'https://raw.githubusercontent.com/QingyuQingxue/index/main/maps.toml'), {
    provider: 'github', username: 'QingyuQingxue', repository: 'index',
  })
  assert.deepEqual(sourceRepository([], 'https://api.atomgit.com/api/v5/repos/CH3NGY2/OC2-Map-Bundle/releases'), map)
  for (const url of ['https://example.com/a/b', 'https://gitcode.com.evil.test/a/b', 'https://user@gitcode.com/a/b', 'https://gitcode.com/a%2Fb/c', 'https://gitcode.com/search/repositories', 'file:///a/b']) {
    assert.equal(sourceRepository([map], url), null, url)
  }
})
