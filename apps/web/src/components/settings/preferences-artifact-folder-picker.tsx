import type { ArtifactStoragePageState } from '@/components/settings/use-artifact-storage-page-state'
import LocalFolderPickerDialog from '@/components/LocalFolderPickerDialog'

export default function ArtifactFolderPicker({
  state,
}: {
  state: ArtifactStoragePageState
}) {
  const {
    artifactDirPickerOpen,
    canBrowseLocalFs,
    setArtifactDirPickerOpen,
    storageForm,
  } = state
  return (
    <LocalFolderPickerDialog
      open={artifactDirPickerOpen}
      onOpenChange={setArtifactDirPickerOpen}
      enabled={canBrowseLocalFs}
      initialPath={storageForm.getValues('local_base_dir')}
      title="Browse artifact folder"
      description="Select a folder on the daemon host where artifact files will be stored."
      selectCurrentLabel="Use current folder"
      selectDirectoryLabel="Select folder"
      onSelectPath={(path) => {
        storageForm.setValue('local_base_dir', path, {
          shouldDirty: true,
          shouldTouch: true,
          shouldValidate: true,
        })
      }}
    />
  )
}
