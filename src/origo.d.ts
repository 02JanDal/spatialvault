declare module "Origo" {
  import type Collection from "ol/Collection";
  import type Feature from "ol/Feature";
  import type Geometry from "ol/geom/Geometry";
  import type Map from "ol/Map";
  import type Overlay from "ol/Overlay";
  import type VectorLayer from "ol/layer/Vector";
  import type VectorSource from "ol/source/Vector";
  import type { Extent } from "ol/extent";

  type StyleLike = string | Record<string, string | number>;
  type AnyRecord = Record<string, any>;

  export interface Eventer {
    on(type: string, listener: (evt?: any) => void): void;
    un(type: string, listener: (evt?: any) => void): void;
    dispatch(type: string, data?: any): void;
  }

  export type EventHandler<T> = (evt?: T) => void;

  export interface TypedEventer<EventMap> {
    on<K extends keyof EventMap>(
      type: K,
      listener: EventHandler<EventMap[K]>,
    ): void;
    un<K extends keyof EventMap>(
      type: K,
      listener: EventHandler<EventMap[K]>,
    ): void;
    dispatch<K extends keyof EventMap>(type: K, data?: EventMap[K]): void;
  }

  export interface OrigoAppEventMap {
    load: Viewer;
  }

  export interface ButtonEventMap {
    change: {
      state?: string;
      text?: string;
      icon?: string;
    };
    update: void;
    click: void;
    mouseenter: void;
    render: void;
    clear: void;
    removeMouseenter: void;
  }

  export interface CollapseEventMap {
    render: void;
    "collapse:toggle": void;
    "collapse:collapse": void;
  }

  export interface DropdownEventMap {
    render: void;
  }

  export interface InputEventMap {
    change: { value: string };
    focusout: { value: string };
  }

  export interface InputRangeEventMap {
    change: { value: string };
  }

  export interface ModalEventMap {
    render: void;
    closed: void;
  }

  export interface PopupMenuEventMap {
    render: void;
  }

  export interface TextareaEventMap {
    change: { value: string };
  }

  export interface UiBase {
    addComponent(child: UiComponent): UiComponent;
    addComponents(children: UiComponent[]): void;
    clearComponents(): void;
    getComponents(): UiComponent[];
    getId(): string;
    removeComponent(component: UiComponent): void;
  }

  export type UiComponent = Eventer &
    UiBase & {
      onInit?: () => void;
      onAdd?: (evt?: any) => void;
      onRender?: () => void;
      render?: () => string | HTMLElement | DocumentFragment;
      [key: string]: any;
    };

  export interface ComponentOptions extends Partial<UiComponent> {
    [key: string]: any;
  }

  export interface ElementOptions {
    target?: HTMLElement;
    cls?: string;
    components?: UiComponent[];
    innerHTML?: string;
    attributes?: Record<string, any>;
    style?: StyleLike;
    tagName?: string;
  }

  export interface ElementComponent extends UiComponent {
    getTarget(): HTMLElement | undefined;
    setTarget(target: HTMLElement): void;
    render(): string | HTMLElement;
  }

  export interface ButtonOptions {
    icon?: string;
    state?: string;
    text?: string;
    cls?: string;
    methods?: Record<string, (el: HTMLElement) => void>;
    data?: Record<string, any>;
    iconCls?: string;
    iconStyle?: Record<string, string | number>;
    click?: (evt?: any) => void;
    mouseenter?: (evt?: any) => void;
    style?: StyleLike;
    textCls?: string;
    tooltipText?: string;
    title?: string;
    tooltipPlacement?: string;
    validStates?: string[];
    ariaLabel?: string;
    tabIndex?: number;
  }

  export interface ButtonComponent
    extends UiComponent, TypedEventer<ButtonEventMap> {
    data?: Record<string, any>;
    getState(): string;
    setState(state: string): void;
    setIcon(icon: string): void;
  }

  export interface CollapseOptions {
    expanded?: boolean;
    bubble?: boolean;
    cls?: string;
    collapseX?: boolean;
    collapseY?: boolean;
    contentComponent: UiComponent;
    headerComponent?: UiComponent;
    footerComponent?: UiComponent;
    contentCls?: string;
    contentStyle?: StyleLike;
    data?: Record<string, any>;
    style?: StyleLike;
    tagName?: string;
    containerCls?: string;
    mainCls?: string;
  }

  export interface CollapseComponent
    extends UiComponent, TypedEventer<CollapseEventMap> {
    containerId: string;
    data?: Record<string, any>;
    expand(): void;
    collapse(): void;
    toggle(evt: Event): void;
  }

  export interface CollapseHeaderOptions {
    cls?: string;
    icon?: string;
    style?: StyleLike;
    title?: string;
  }

  export type DropdownItem = { label: string; value: any } | string;

  export interface DropdownOptions {
    cls?: string;
    containerCls?: string;
    contentCls?: string;
    contentStyle?: StyleLike;
    buttonCls?: string;
    buttonIconCls?: string;
    buttonContainerCls?: string;
    style?: StyleLike;
    direction?: "up" | "down" | string;
    ariaLabel?: string;
    buttonTextCls?: string;
    text?: string;
    items?: DropdownItem[];
  }

  export interface DropdownComponent
    extends UiComponent, TypedEventer<DropdownEventMap> {
    setButtonText(text: string): void;
    getItems(): UiComponent[] | false;
    setItems(items: DropdownItem[]): void;
    selectItem(itemEl: UiComponent, doClick?: boolean): void;
    toggle(): void;
  }

  export interface FloatingPanelOptions {
    closeIcon?: string;
    title?: string;
    viewer: Viewer;
    type?: "floating" | "left" | string;
    removeOnClose?: boolean;
    contentComponent?: UiComponent;
    isActive?: boolean;
  }

  export interface FloatingPanelComponent extends UiComponent {
    hide(): void;
    show(): void;
    remove(): void;
    getStatus(): boolean;
    changeContent(component: UiComponent, title?: string): void;
    getContentElement(): HTMLElement | null;
  }

  export interface IconOptions {
    icon: string;
    cls?: string;
    title?: string;
    style?: StyleLike;
  }

  export interface IconComponent extends UiComponent {
    update(): void;
    setIcon(icon: string): void;
  }

  export interface InputOptions {
    cls?: string;
    placeholderText?: string;
    style?: StyleLike;
    value?: string;
  }

  export interface InputComponent
    extends UiComponent, TypedEventer<InputEventMap> {
    getValue(): string;
  }

  export interface InputRangeOptions {
    cls?: string;
    minValue?: number;
    maxValue?: number;
    initialValue?: number;
    step?: number;
    style?: StyleLike;
    unit?: string;
    label?: string;
  }

  export interface InputRangeComponent
    extends UiComponent, TypedEventer<InputRangeEventMap> {
    setValue(value: number | string): void;
  }

  export interface InputFileOptions {
    labelCls?: string;
    inputCls?: string;
    label?: string;
    change?: (evt: Event) => void;
  }

  export interface TextareaOptions {
    cls?: string;
    placeholderText?: string;
    rows?: number;
    cols?: number;
    style?: StyleLike;
    value?: string;
  }

  export interface TextareaComponent
    extends UiComponent, TypedEventer<TextareaEventMap> {
    getValue(): string;
  }

  export interface ModalOptions {
    title?: string;
    content?: string;
    contentElement?: HTMLElement;
    contentCmp?: UiComponent;
    cls?: string;
    contentCls?: string;
    static?: boolean;
    target: string;
    closeIcon?: string;
    style?: string;
    newTabUrl?: string;
  }

  export interface ModalComponent
    extends UiComponent, TypedEventer<ModalEventMap> {
    closeModal(): void;
    hide(): void;
    show(): void;
  }

  export interface PopupMenuOptions {
    onUnfocus?: (evt: MouseEvent) => void;
    style?: string;
    cls?: string;
  }

  export interface PopupMenuComponent
    extends UiComponent, TypedEventer<PopupMenuEventMap> {
    getEl(): HTMLElement | null;
    setPosition(
      position: Partial<Record<"top" | "bottom" | "left" | "right", string>>,
    ): void;
    getVisibility(): boolean;
    setVisibility(visible: boolean): void;
    toggleVisibility(): void;
    setContent(content: string): void;
  }

  export interface SlidenavOptions {
    backIcon?: string;
    cls?: string;
    mainComponent?: UiComponent;
    secondaryComponent?: UiComponent;
    style?: StyleLike;
    legendSlideNav?: boolean;
    viewer?: Viewer;
  }

  export interface SlidenavComponent extends UiComponent {
    slideToMain(): void;
    slideToSecondary(): void;
    setMain(component: UiComponent): UiComponent;
    setSecondary(component: UiComponent): UiComponent;
    getState(): string;
  }

  export interface ToggleGroupOptions {
    cls?: string;
    components?: UiComponent[];
    style?: StyleLike;
    tagName?: string;
  }

  export interface OrigoDom {
    createElement(
      type: string,
      content?: string | UiComponent | HTMLElement | DocumentFragment,
      options?: Record<string, any>,
    ): HTMLElement;
    createStyle(styleSettings: StyleLike): string;
    html(htmlString: string): DocumentFragment;
    matches(match: string, parent: Element, target: Element): boolean;
    replace(el: Element, htmlString: string): void;
  }

  export interface OrigoUI {
    dom: OrigoDom;
    Button(options?: ButtonOptions): ButtonComponent;
    Collapse(options: CollapseOptions): CollapseComponent;
    CollapseHeader(options?: CollapseHeaderOptions): UiComponent;
    Dropdown(options?: DropdownOptions): DropdownComponent;
    FloatingPanel(options: FloatingPanelOptions): FloatingPanelComponent;
    Icon(options: IconOptions): IconComponent;
    Element(options?: ElementOptions): ElementComponent;
    Input(options?: InputOptions): InputComponent;
    InputRange(options?: InputRangeOptions): InputRangeComponent;
    InputFile(options?: InputFileOptions): UiComponent;
    Textarea(options?: TextareaOptions): TextareaComponent;
    Modal(options: ModalOptions): ModalComponent;
    PopupMenu(options?: PopupMenuOptions): PopupMenuComponent;
    Slidenav(options: SlidenavOptions): SlidenavComponent;
    ToggleGroup(options?: ToggleGroupOptions): UiComponent;
    Component(options: ComponentOptions): UiComponent;
    Eventer(): Eventer;
    cuid(): string;
  }

  export interface OrigoOl {
    geom: typeof import("ol/geom");
    interaction: typeof import("ol/interaction");
    layer: typeof import("ol/layer");
    source: typeof import("ol/source");
    style: typeof import("ol/style");
    format: typeof import("ol/format");
    proj: typeof import("ol/proj");
    Feature: typeof import("ol/Feature").default;
    Collection: typeof import("ol/Collection").default;
    Overlay: typeof import("ol/Overlay").default;
  }

  export interface FeatureLayerApi {
    addFeature(feature: Feature): void;
    removeFeature(feature: Feature): void;
    setSourceLayer(layer: any): void;
    getFeatures(): Feature[];
    getFeatureLayer(): VectorLayer<any>;
    getFeatureStore(): VectorSource<any>;
    getSourceLayer(): any;
  }

  export interface OrigoLoader {
    show(): void;
    hide(): void;
    withLoading<T>(cb: () => Promise<T>): Promise<T>;
    getInlineSpinner(): HTMLElement;
  }

  export type ControlFactory = (
    options?: AnyRecord,
  ) =>
    | UiComponent
    | ((options?: AnyRecord) => UiComponent)
    | Promise<UiComponent | ((options?: AnyRecord) => UiComponent)>;
  export type ExtensionFactory = (options?: AnyRecord) => AnyRecord;

  export interface OrigoConfig {
    controls: Array<{ name: string; options?: AnyRecord } | AnyRecord>;
    featureinfoOptions: AnyRecord;
    crossDomain: boolean;
    target: string;
    keyboardEventTarget: EventTarget;
    svgSpritePath: string;
    svgSprites: string[];
    breakPoints: Record<string, [number, number]>;
    breakPointsPrefix: string;
    defaultControls: Array<{ name: string; options?: AnyRecord }>;
    [key: string]: any;
  }

  export interface OrigoOptions extends Partial<OrigoConfig> {
    controls?: Record<string, ControlFactory>;
    baseUrl?: string;
  }

  export interface ViewerOptions extends AnyRecord {
    breakPoints?: Record<string, [number, number]>;
    breakPointsPrefix?: string;
    clsOptions?: string;
    consoleId?: string;
    mapCls?: string;
    controls?: UiComponent[];
    featureinfoOptions?: AnyRecord;
    groups?: AnyRecord[];
    pageSettings?: AnyRecord;
    projectionCode?: string;
    projectionExtent?: Extent;
    startExtent?: Extent;
    extent?: Extent;
    center?: [number, number];
    zoom?: number;
    resolutions?: number[] | null;
    layers?: AnyRecord[];
    layerParams?: Record<string, AnyRecord>;
    map?: string;
    params?: AnyRecord;
    proj4Defs?: Record<string, string>;
    styles?: Record<string, AnyRecord>;
    source?: Record<string, AnyRecord>;
    clusterOptions?: AnyRecord;
    tileGridOptions?: AnyRecord;
    loggerOptions?: AnyRecord;
    url?: string;
    palette?: AnyRecord;
    projection?: AnyRecord;
  }

  export interface LegendState {
    expanded: boolean;
    visibleLayersViewActive: boolean;
  }

  export interface LegendOverlaysComponent extends UiComponent {
    overlaysCollapse: CollapseComponent;
    slidenav: SlidenavComponent;
    getGroups(): UiComponent[];
    getOverlays(): any[];
  }

  export interface LegendControl extends UiComponent {
    getLayerSwitcherCmp(): UiComponent | undefined;
    getState(): LegendState;
    restoreState(params: AnyRecord): void;
    getuseGroupIndication(): boolean;
    getOverlaysCollapse(): CollapseComponent;
    getOverlays(): LegendOverlaysComponent;
    setVisibleLayersViewActive(active: boolean): void;
    addButtonToTools(button: UiComponent, buttonGroup?: string): void;
    hide(): void;
    unhide(): void;
  }

  export interface SharemapControl extends UiComponent {
    addParamsToGetMapState(
      key: string,
      callback: (state: AnyRecord) => AnyRecord | void,
    ): void;
  }

  export interface Viewer extends UiComponent {
    addControl(control: UiComponent): void;
    addControls(): void;
    addGroup(groupProps: AnyRecord): void;
    addGroups(groupsProps: AnyRecord[]): void;
    addLayer(layerProps: AnyRecord, insertBefore?: any): any;
    addLayers(layersProps: AnyRecord[]): void;
    addSource(sourceName: string, sourceProps: AnyRecord): void;
    addStyle(styleName: string, styleProps: AnyRecord): void;
    addMarker(
      coordinates: number[],
      title: string,
      content: string,
      layerProps?: AnyRecord,
      showPopup?: boolean,
    ): void;
    getBreakPoints(
      size?: string,
    ): [number, number] | Record<string, [number, number]>;
    getCenter(): (
      geometry: Geometry,
      destination?: string,
      axisOrientation?: string,
      map?: Map,
    ) => number[] | undefined;
    getClusterOptions(): AnyRecord;
    getConsoleId(): string;
    getControlByName(name: "legend"): LegendControl | null;
    getControlByName(name: "sharemap"): SharemapControl | null;
    getControlByName(name: string): UiComponent | null;
    getExtent(): Extent | number[];
    getFeatureinfo(): AnyRecord;
    getFooter(): UiComponent;
    getGroup(groupName: string): AnyRecord | undefined;
    getGroups(): AnyRecord[];
    getInitialZoom(): number;
    getLayer(layerName: string): any | undefined;
    getLayerStylePicker(layer: any): AnyRecord[];
    getLayers(): any[];
    getLayersByProperty(key: string, val: any, byName?: boolean): any[];
    getMain(): UiComponent;
    getMap(): Map;
    getMapName(): string | undefined;
    getMapSource(): Record<string, AnyRecord>;
    getMapUrl(): string;
    getMapUtils(): AnyRecord;
    getUtils(): AnyRecord;
    getProjection(): AnyRecord;
    getProjectionCode(): string | undefined;
    getQueryableLayers(includeImageFeatureInfoMode?: boolean): any[];
    getGroupLayers(): any[];
    getResolutions(): number[] | null;
    getSearchableLayers(searchableDefault?: boolean | "always"): string[];
    getSize(): AnyRecord;
    getSource(name: string): AnyRecord;
    getStyle(styleName: string): AnyRecord | null;
    getStyles(): Record<string, AnyRecord>;
    getTarget(): string;
    getTileGrid(): AnyRecord;
    getTileGridSettings(): AnyRecord;
    getTileSize(): [number, number];
    getUrl(): string | undefined;
    getUrlParams(): Record<string, AnyRecord>;
    getViewerOptions(): ViewerOptions;
    removeGroup(groupName: string): void;
    removeLayer(layer: any): void;
    removeOverlays(overlays?: Overlay | Overlay[] | Collection<Overlay>): void;
    removeMarkers(layerName: string): void;
    setStyle(styleName: string, style: AnyRecord): void;
    zoomToExtent(geometry: Geometry, level?: number): Extent | false;
    getSelectionManager(): AnyRecord;
    getStylewindow(): AnyRecord;
    getEmbedded(): boolean;
    permalink: AnyRecord;
    generateUUID(): string;
    centerMarker: UiComponent;
    getLogger(): UiComponent;
  }

  export interface OrigoApi {
    (): Viewer | undefined;
    controls(): Record<string, ControlFactory>;
    extensions(): Record<string, ExtensionFactory>;
  }

  export interface OrigoApp
    extends UiComponent, TypedEventer<OrigoAppEventMap> {
    api: OrigoApi;
    getConfig(): OrigoConfig;
  }

  export interface OrigoDropdownOptions {
    dataAttribute: string;
    active?: any;
  }

  export interface OrigoDropdownItem {
    name: string;
    value: any;
  }

  export interface OrigoDropdownApi {
    select(value: any): void;
  }

  export interface OrigoStatic {
    (configPath: string, options?: OrigoOptions): OrigoApp | null;
    controls: Record<string, ControlFactory>;
    extensions: Record<string, ExtensionFactory>;
    ui: OrigoUI;
    Style: AnyRecord;
    featurelayer(features?: Feature | Feature[], map?: Map): FeatureLayerApi;
    getFeatureInfo: (...args: any[]) => any;
    getFeature: (...args: any[]) => any;
    ol: OrigoOl;
    Utils: AnyRecord;
    dropdown(
      target: string,
      items: OrigoDropdownItem[],
      options: OrigoDropdownOptions,
    ): OrigoDropdownApi;
    renderSvgIcon: (
      styleRule: AnyRecord[],
      options?: { opacity?: number },
    ) => string;
    SelectedItem: new (...args: any[]) => any;
    Loader: OrigoLoader;
    layerType: Record<string, string>;
    mapUtils: AnyRecord;
  }

  const Origo: OrigoStatic;
  export default Origo;
}
