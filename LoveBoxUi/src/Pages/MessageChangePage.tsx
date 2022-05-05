import { Alert, Grid, Snackbar, Typography } from "@mui/material";
import { FC, useState } from "react";
import MessageTextField from "../Components/MessageTextField";
import PhotoGallery from "../Components/PhotoGallery";

const colorSelectPageStyle = {
  height: "100%",
  margin: "0px",
  minHeight: "100vh",
  backgroundColor: "#222222",
};

const gridContainerStyle = {
  marginTop: "0px",
  paddingBottom: "40px",
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
};

const gridItemStyle = {
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
};

const titleStyle = {
  color: "white",
  fontSize: "40px",
  marginTop: "20px",
  fontFamily: "Ubuntu, -apple-system",
  fontWeight: "bold",
};

const MessageChangePage: FC = () => {
  const [noFontSnackBarOpen, setNoFontSnackBarOpen] = useState<boolean>(false);
  const [successSnackBarOpen, setSuccessSnackBarOpen] =
    useState<boolean>(false);
  const [errorSnackBarOpen, setErrorSnackBarOpen] = useState<boolean>(false);

  const handleNoFontSnackBarClose = () => {
    setNoFontSnackBarOpen(false);
  };

  const handleSuccessSnackBarClose = () => {
    setSuccessSnackBarOpen(false);
  };

  const handleErrorSnackBarClose = () => {
    setErrorSnackBarOpen(false);
  };

  return (
    <div style={colorSelectPageStyle}>
      <Grid container spacing={2} style={gridContainerStyle}>
        <Grid item xs={12} style={gridItemStyle}>
          <Typography style={titleStyle}>Love Box</Typography>
        </Grid>
        <Grid item xs={12} style={gridItemStyle}>
          <MessageTextField
            setNoFontSnackBarOpen={setNoFontSnackBarOpen}
            setSuccessSnackBarOpen={setSuccessSnackBarOpen}
            setErrorSnackBarOpen={setErrorSnackBarOpen}
          />
        </Grid>
        <Grid item xs={12} style={gridItemStyle}>
          <PhotoGallery
            setSuccessSnackBarOpen={setSuccessSnackBarOpen}
            setErrorSnackBarOpen={setErrorSnackBarOpen}
          />
        </Grid>
      </Grid>
      <Snackbar
        open={noFontSnackBarOpen}
        autoHideDuration={6000}
        onClose={handleNoFontSnackBarClose}
      >
        <Alert
          onClose={handleNoFontSnackBarClose}
          severity="error"
          sx={{ width: "100%" }}
        >
          No font size set!
        </Alert>
      </Snackbar>
      <Snackbar
        open={successSnackBarOpen}
        autoHideDuration={6000}
        onClose={handleSuccessSnackBarClose}
      >
        <Alert
          onClose={handleSuccessSnackBarClose}
          severity="success"
          sx={{ width: "100%" }}
        >
          Success!
        </Alert>
      </Snackbar>
      <Snackbar
        open={errorSnackBarOpen}
        autoHideDuration={6000}
        onClose={handleErrorSnackBarClose}
      >
        <Alert
          onClose={handleErrorSnackBarClose}
          severity="error"
          sx={{ width: "100%" }}
        >
          Error: Check console for error details.
        </Alert>
      </Snackbar>
    </div>
  );
};

export default MessageChangePage;
