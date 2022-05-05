export interface MessageChangeRequest {
  fontsize?: number;
  image: boolean;
  message: string;
  modified?: string;
}

export interface ImageChangeRequest {
  image: boolean;
  message: string;
}

export interface MessagePreviewRequest {
  fontsize: number;
  message: string;
}

export interface Photo {
  src: string;
  srcSet?: string | string[] | undefined;
  sizes?: string | string[] | undefined;
  width: number;
  height: number;
  alt?: string | undefined;
  key?: string | undefined;
}
