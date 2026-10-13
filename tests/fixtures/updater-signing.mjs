import { createHash, generateKeyPairSync, randomBytes, sign } from 'node:crypto'

export function signingFixture(defaultVersion) {
  const { publicKey, privateKey } = generateKeyPairSync('ed25519')
  const keyId = randomBytes(8)
  const key = Buffer.concat([Buffer.from('Ed'), keyId, publicKey.export({ type: 'spki', format: 'der' }).subarray(-32)])
  const encodedPublicKey = Buffer.from(`untrusted comment: public test key\n${key.toString('base64')}\n`).toString('base64')
  const signatureFor = (bytes, version = defaultVersion, algorithm = 'ED') => {
    const message = algorithm === 'ED' ? createHash('blake2b512').update(bytes).digest() : bytes
    const raw = sign(null, message, privateKey)
    const comment = `timestamp:1\tfile:fixture\tversion:${version}`
    const global = sign(null, Buffer.concat([raw, Buffer.from(comment)]), privateKey)
    return Buffer.from(`untrusted comment: public test signature\n${Buffer.concat([Buffer.from(algorithm), keyId, raw]).toString('base64')}\ntrusted comment: ${comment}\n${global.toString('base64')}\n`).toString('base64')
  }
  return { encodedPublicKey, signatureFor }
}
