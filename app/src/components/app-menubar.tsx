import {
  Menubar,
  MenubarContent,
  MenubarItem,
  MenubarMenu,
  MenubarSeparator,
  MenubarShortcut,
  MenubarTrigger,
} from "@/components/ui/menubar";

type MenuAction = () => void;

export function AppMenubar(props: {
  onNewProject: MenuAction;
  onOpenProject: MenuAction;
  onNewFile: MenuAction;
  onNewFolder: MenuAction;
  onImport: MenuAction;
  onSettings: MenuAction;
  onUndo: MenuAction;
  onRedo: MenuAction;
  onToggleTree: MenuAction;
  onToggleFlat: MenuAction;
  onToggleInspector: MenuAction;
  onPalette: MenuAction;
  onRescan: MenuAction;
}) {
  const mod = navigator.platform.includes("Mac") ? "⌘" : "Ctrl+";
  return (
    <Menubar className="h-7 rounded-none border-x-0 border-t-0 px-1">
      <MenubarMenu>
        <MenubarTrigger className="px-2 py-0.5 text-xs">File</MenubarTrigger>
        <MenubarContent>
          <MenubarItem onClick={props.onNewProject}>New Project…</MenubarItem>
          <MenubarItem onClick={props.onOpenProject}>
            Open Project… <MenubarShortcut>{mod}O</MenubarShortcut>
          </MenubarItem>
          <MenubarItem onClick={props.onNewFile}>
            New File <MenubarShortcut>{mod}N</MenubarShortcut>
          </MenubarItem>
          <MenubarItem onClick={props.onNewFolder}>
            New Folder <MenubarShortcut>{mod}⇧N</MenubarShortcut>
          </MenubarItem>
          <MenubarSeparator />
          <MenubarItem onClick={props.onImport}>
            Import… <MenubarShortcut>{mod}I</MenubarShortcut>
          </MenubarItem>
          <MenubarSeparator />
          <MenubarItem onClick={props.onSettings}>
            Settings… <MenubarShortcut>{mod},</MenubarShortcut>
          </MenubarItem>
        </MenubarContent>
      </MenubarMenu>
      <MenubarMenu>
        <MenubarTrigger className="px-2 py-0.5 text-xs">Edit</MenubarTrigger>
        <MenubarContent>
          <MenubarItem onClick={props.onUndo}>
            Undo <MenubarShortcut>{mod}Z</MenubarShortcut>
          </MenubarItem>
          <MenubarItem onClick={props.onRedo}>
            Redo <MenubarShortcut>{mod}⇧Z</MenubarShortcut>
          </MenubarItem>
        </MenubarContent>
      </MenubarMenu>
      <MenubarMenu>
        <MenubarTrigger className="px-2 py-0.5 text-xs">View</MenubarTrigger>
        <MenubarContent>
          <MenubarItem onClick={props.onToggleTree}>Toggle Tree Panel</MenubarItem>
          <MenubarItem onClick={props.onToggleFlat}>Toggle Flat Panel</MenubarItem>
          <MenubarItem onClick={props.onToggleInspector}>Toggle Inspector</MenubarItem>
          <MenubarSeparator />
          <MenubarItem onClick={props.onPalette}>
            Command Palette… <MenubarShortcut>{mod}K</MenubarShortcut>
          </MenubarItem>
          <MenubarSeparator />
          <MenubarItem onClick={props.onRescan}>
            Rescan Project <MenubarShortcut>{mod}⇧R</MenubarShortcut>
          </MenubarItem>
        </MenubarContent>
      </MenubarMenu>
    </Menubar>
  );
}
