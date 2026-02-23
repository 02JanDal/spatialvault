import { createSignal, Show } from "solid-js";
import {
  Button,
  Collapse,
  CollapseHeader,
  Dropdown,
  Input,
  InputRange,
  PopupMenu,
  Textarea,
  ToggleGroup,
} from "./components/origo";
import type { Viewer } from "Origo";

type TreeNode = { id: string; label: string; children: TreeNode[] };

function TreeNodeItem({
  id,
  label,
  children,
  path,
  onSelected,
  selected,
}: TreeNode & {
  path: string[];
  onSelected: (nodePath: string[]) => void;
  selected: string[] | null;
}) {
  const [expanded, setExpanded] = createSignal(path.length < 2);

  return (
    <Collapse
      cls=""
      expanded={expanded()}
      collapseX={false}
      onToggle={() => onSelected([...path, id])}
      header={
        <div
          class="flex row align-center padding-left text-smaller pointer collapse-header item wrap"
          style="width:100%;padding-right:0.275rem"
        >
          <div class="flex row align-center grow basis-0">
            <Button
              cls="icon-small compact round"
              iconCls="rotate grey"
              style={{ "align-self": "flex-start" }}
              icon="#ic_chevron_right_24px"
              onClick={() => setExpanded((prev) => !prev)}
            />
            <span class="grow padding-x-small" style="overflow-wrap:anywhere;">
              {label}
            </span>
          </div>
          <Button
            cls="round small icon-smaller no-shrink"
            icon="#ic_more_vert_24px"
            tabIndex={-1}
            style={{ "align-self": "center" }}
          />
        </div>
      }
    >
      <Show when={children.length > 0}>
        <ul class="divider-start padding-left padding-top-small">
          {children.map((child) => (
            <li>
              <TreeNodeItem
                {...child}
                path={[...path, id]}
                onSelected={onSelected}
                selected={selected}
              />
            </li>
          ))}
        </ul>
      </Show>
    </Collapse>
  );
}

function Catalog({ viewer }: { viewer: Viewer }) {
  const userTree: TreeNode[] = [
    {
      id: "mine",
      label: "Mine",
      children: [
        {
          id: "omr1",
          label: "Område 1",
          children: [],
        },
        {
          id: "omr2",
          label: "Område 2",
          children: [
            {
              id: "delomr1",
              label: "Delområde 1",
              children: [],
            },
            {
              id: "delomr2",
              label: "Delområde 2",
              children: [],
            },
          ],
        },
      ],
    },
    {
      id: "shared",
      label: "Shared with me",
      children: [
        {
          id: "user1",
          label: "User 1",
          children: [],
        },
        {
          id: "user2",
          label: "User 2",
          children: [],
        },
      ],
    },
  ];

  const [selected, setSelected] = createSignal<null | string[]>(null);

  return (
    <div class="catalog-container">
      <div class="catalog-sidebar">
        {userTree.map((node) => (
          <TreeNodeItem
            {...node}
            path={[]}
            selected={selected()}
            onSelected={setSelected}
          />
        ))}
      </div>
      <div class="catalog-page">Page</div>
    </div>
  );
}

export default Catalog;
