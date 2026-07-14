import type { PreferencesPageState } from '@/routes/settings/preferences'
import LocalFolderPickerDialog from '@/components/LocalFolderPickerDialog'

export function ArtifactFolderPicker({
  state,
}: {
  state: PreferencesPageState
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
      title="Browse Artifact Folder"
      description="Select a folder on the daemon host where artifact files will be stored."
      selectCurrentLabel="Use Current Folder"
      selectDirectoryLabel="Select Folder"
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
