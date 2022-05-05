import { Grid, IconButton } from "@mui/material";
import { Dispatch, FC, SetStateAction, useEffect, useState } from "react";
import Gallery from "react-photo-gallery";
import config from "../config";
import { photos } from "../images/photos";
import { ImageChangeRequest, Photo } from "../types";
import ShuffleIcon from "@mui/icons-material/Shuffle";

interface IPhotoGallery {
  setSuccessSnackBarOpen: Dispatch<SetStateAction<boolean>>;
  setErrorSnackBarOpen: Dispatch<SetStateAction<boolean>>;
}

const photoGalleryGridContainerStyle = {
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  height: "auto",
  backgroundColor: "#3B3B3B",
  width: "95%",
  borderRadius: "10px",
  padding: "5px",
};

const photoGalleryItemStyle = {
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
};

const iconStyle = {
  color: "white",
  fontSize: "50px",
};

const PhotoGallery: FC<IPhotoGallery> = (props): JSX.Element => {
  const [photoList, setPhotoList] = useState<Photo[]>();

  useEffect(() => {
    shufflePhotos();
  }, []);

  const onPhotoClick = (photoIndex: number) => {
    console.log((photoList as Photo[])[photoIndex].key as string);
    const imageChangeRequest: ImageChangeRequest = {
      image: true,
      message: (photoList as Photo[])[photoIndex].key as string,
    };

    fetch(config.EINK_CHANGE_PICTURE_API, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(imageChangeRequest),
    })
      .catch((error) => {
        console.log("ERROR", error);
        props.setErrorSnackBarOpen(true);
      })
      .finally(() => props.setSuccessSnackBarOpen(true));
  };

  const shufflePhotos = () => {
    setPhotoList(
      photos
        .map((value) => ({ value, sort: Math.random() }))
        .sort((a, b) => a.sort - b.sort)
        .map(({ value }) => value)
    );
  };

  return (
    <Grid container style={photoGalleryGridContainerStyle}>
      <Grid item xs={12} style={photoGalleryItemStyle}>
        <IconButton onClick={shufflePhotos}>
          <ShuffleIcon style={iconStyle} />
        </IconButton>
      </Grid>
      <Grid item xs={12} style={photoGalleryItemStyle}>
        <Gallery
          onClick={(_event, photo) => onPhotoClick(photo.index)}
          photos={photoList as Photo[]}
        />
      </Grid>
    </Grid>
  );
};

export default PhotoGallery;
