import type { DomainMessages } from '../types.ts'
import { mapPublishAuth } from './auth.ts'
import { mapPublishRepository } from './repository.ts'
import { mapPublishPreparation } from './preparation.ts'
import { mapPublishUpload } from './upload.ts'
import { mapPublishVersions } from './versions.ts'

/** 统一域入口；业务只依赖语言 key，不依赖文案子表的位置。 */
export const mapPublish: DomainMessages = {
  ...mapPublishAuth,
  ...mapPublishRepository,
  ...mapPublishPreparation,
  ...mapPublishUpload,
  ...mapPublishVersions,
}
