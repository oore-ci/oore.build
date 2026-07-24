import {
  Atom as Atom01Icon,
  Bug as Bug01Icon,
  Building2 as Building03Icon,
  ChartNoAxesCombined as ChartLineData01Icon,
  CloudCog as CloudServerIcon,
  SquareTerminal as ComputerTerminal01Icon,
  Cpu as CpuIcon,
  Box as CubeIcon,
  Database as Database01Icon,
  Zap as FlashIcon,
  Globe as Globe02Icon,
  HardDrive as HardDriveIcon,
  House as Home01Icon,
  KeyRound as Key01Icon,
  Layers as Layers01Icon,
  Puzzle as PuzzleIcon,
  Rocket as Rocket01Icon,
  Satellite as Satellite01Icon,
  Server as ServerStack01Icon,
  Shield as Shield01Icon,
  CodeXml as SourceCodeIcon,
  Target as Target01Icon,
  FlaskConical as TestTubeIcon,
  Wifi as Wifi01Icon,
} from 'lucide-react'
import type { LucideIcon } from 'lucide-react'

export interface InstanceIconEntry {
  key: string
  label: string
  icon: LucideIcon
}

export const INSTANCE_ICONS: Array<InstanceIconEntry> = [
  { key: 'cloud-server', label: 'Cloud Server', icon: CloudServerIcon },
  { key: 'server-stack', label: 'Server', icon: ServerStack01Icon },
  { key: 'database', label: 'Database', icon: Database01Icon },
  { key: 'code', label: 'Code', icon: SourceCodeIcon },
  { key: 'terminal', label: 'Terminal', icon: ComputerTerminal01Icon },
  { key: 'rocket', label: 'Rocket', icon: Rocket01Icon },
  { key: 'globe', label: 'Globe', icon: Globe02Icon },
  { key: 'shield', label: 'Shield', icon: Shield01Icon },
  { key: 'cpu', label: 'CPU', icon: CpuIcon },
  { key: 'hard-drive', label: 'Hard Drive', icon: HardDriveIcon },
  { key: 'wifi', label: 'WiFi', icon: Wifi01Icon },
  { key: 'satellite', label: 'Satellite', icon: Satellite01Icon },
  { key: 'building', label: 'Building', icon: Building03Icon },
  { key: 'home', label: 'Home', icon: Home01Icon },
  { key: 'test-tube', label: 'Test Tube', icon: TestTubeIcon },
  { key: 'atom', label: 'Atom', icon: Atom01Icon },
  { key: 'flash', label: 'Flash', icon: FlashIcon },
  { key: 'target', label: 'Target', icon: Target01Icon },
  { key: 'puzzle', label: 'Puzzle', icon: PuzzleIcon },
  { key: 'layers', label: 'Layers', icon: Layers01Icon },
  { key: 'cube', label: 'Cube', icon: CubeIcon },
  { key: 'chart', label: 'Chart', icon: ChartLineData01Icon },
  { key: 'bug', label: 'Bug', icon: Bug01Icon },
  { key: 'key', label: 'Key', icon: Key01Icon },
]

export const DEFAULT_INSTANCE_ICON_KEY = 'cloud-server'

const iconMap = new Map(INSTANCE_ICONS.map((entry) => [entry.key, entry.icon]))

export function getInstanceIcon(key: string | undefined): LucideIcon {
  return iconMap.get(key ?? '') ?? CloudServerIcon
}
