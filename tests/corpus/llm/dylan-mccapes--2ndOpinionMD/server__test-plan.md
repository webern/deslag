# 2ndOpinionMD Express Server Test Plan

This test plan outlines the testing strategy for the 2ndOpinionMD Express server with MongoDB integration. It covers API endpoints, database operations, and end-to-end functionality.

## 1. Environment Setup Tests

### 1.1 MongoDB Connection
- **Test Case**: Verify MongoDB connection
- **Steps**:
  1. Start MongoDB service
  2. Start Express server
  3. Check server logs for successful connection message
- **Expected Result**: Server logs show "MongoDB Connected: localhost"

### 1.2 Environment Variables
- **Test Case**: Verify environment variables are loaded correctly
- **Steps**:
  1. Set environment variables in .env file
  2. Start Express server
  3. Check server logs for environment variable usage
- **Expected Result**: Server uses correct environment variables

## 2. Authentication Tests

### 2.1 User Registration
- **Test Case**: Register a new user
- **Steps**:
  1. Send POST request to `/api/auth/register` with user data
  2. Verify response contains token and user data
  3. Check MongoDB for user record
- **Expected Result**: User is created in MongoDB with hashed password

### 2.2 User Login
- **Test Case**: Login with registered user
- **Steps**:
  1. Send POST request to `/api/auth/login` with credentials
  2. Verify response contains token and user data
- **Expected Result**: Valid JWT token is returned

### 2.3 Invalid Login
- **Test Case**: Login with invalid credentials
- **Steps**:
  1. Send POST request to `/api/auth/login` with incorrect credentials
  2. Verify response contains error message
- **Expected Result**: 401 Unauthorized response

### 2.4 Protected Route Access
- **Test Case**: Access protected route with valid token
- **Steps**:
  1. Login to get token
  2. Send GET request to `/api/user/profile` with token
  3. Verify response contains user data
- **Expected Result**: User profile data is returned

### 2.5 Protected Route Access Without Token
- **Test Case**: Access protected route without token
- **Steps**:
  1. Send GET request to `/api/user/profile` without token
  2. Verify response contains error message
- **Expected Result**: 401 Unauthorized response

## 3. Diagnosis Tests

### 3.1 Submit Symptoms
- **Test Case**: Submit symptoms for diagnosis
- **Steps**:
  1. Login to get token
  2. Send POST request to `/api/diagnose` with symptom data
  3. Verify response contains diagnostic results
  4. Check MongoDB for report record
- **Expected Result**: Diagnostic results are returned and stored in MongoDB

### 3.2 Submit Symptoms with Missing Fields
- **Test Case**: Submit symptoms with missing required fields
- **Steps**:
  1. Login to get token
  2. Send POST request to `/api/diagnose` with incomplete data
  3. Verify response contains error message
- **Expected Result**: 400 Bad Request response with missing fields listed

## 4. PDF Generation Tests

### 4.1 Generate PDF from Report ID
- **Test Case**: Generate PDF from existing report
- **Steps**:
  1. Login to get token
  2. Create a diagnosis report
  3. Send POST request to `/api/generate-pdf` with report ID
  4. Verify response contains PDF data
  5. Check MongoDB for updated report record with PDF URL
- **Expected Result**: PDF is generated and report is updated with PDF URL

### 4.2 Generate PDF from Diagnostic Results
- **Test Case**: Generate PDF from diagnostic results
- **Steps**:
  1. Login to get token
  2. Send POST request to `/api/generate-pdf` with diagnostic results
  3. Verify response contains PDF data
- **Expected Result**: PDF is generated

### 4.3 Generate PDF with Invalid Report ID
- **Test Case**: Generate PDF with invalid report ID
- **Steps**:
  1. Login to get token
  2. Send POST request to `/api/generate-pdf` with invalid report ID
  3. Verify response contains error message
- **Expected Result**: 404 Not Found response

## 5. Report Management Tests

### 5.1 Get User Reports
- **Test Case**: Get all reports for a user
- **Steps**:
  1. Login to get token
  2. Create multiple diagnosis reports
  3. Send GET request to `/api/reports`
  4. Verify response contains all reports
- **Expected Result**: All user reports are returned

### 5.2 Get User Reports with No Reports
- **Test Case**: Get reports for a user with no reports
- **Steps**:
  1. Register a new user
  2. Login to get token
  3. Send GET request to `/api/reports`
  4. Verify response contains empty reports array
- **Expected Result**: Empty reports array is returned

## 6. Form Field Tests

### 6.1 Get Symptoms
- **Test Case**: Get available symptoms
- **Steps**:
  1. Login to get token
  2. Send GET request to `/api/fields/symptoms`
  3. Verify response contains symptoms array
- **Expected Result**: Symptoms array is returned

### 6.2 Get Prior Diagnoses
- **Test Case**: Get available prior diagnoses
- **Steps**:
  1. Login to get token
  2. Send GET request to `/api/fields/prior-diagnoses`
  3. Verify response contains prior diagnoses array
- **Expected Result**: Prior diagnoses array is returned

### 6.3 Get Sex Options
- **Test Case**: Get available sex options
- **Steps**:
  1. Login to get token
  2. Send GET request to `/api/fields/sex-options`
  3. Verify response contains sex options array
- **Expected Result**: Sex options array is returned

## 7. Database Tests

### 7.1 User Data Persistence
- **Test Case**: Verify user data persists after server restart
- **Steps**:
  1. Register a new user
  2. Restart the server
  3. Login with the registered user
- **Expected Result**: User can login after server restart

### 7.2 Report Data Persistence
- **Test Case**: Verify report data persists after server restart
- **Steps**:
  1. Login to get token
  2. Create a diagnosis report
  3. Restart the server
  4. Get user reports
- **Expected Result**: Report is still available after server restart

### 7.3 MongoDB Schema Validation
- **Test Case**: Verify MongoDB schema validation
- **Steps**:
  1. Attempt to create a user with invalid data
  2. Verify MongoDB validation error
- **Expected Result**: MongoDB validation prevents invalid data

## 8. End-to-End Tests

### 8.1 Complete User Journey
- **Test Case**: Complete user journey from registration to PDF generation
- **Steps**:
  1. Register a new user
  2. Login with the registered user
  3. Submit symptoms for diagnosis
  4. Generate PDF from the diagnosis report
  5. Get user reports
- **Expected Result**: All steps complete successfully

### 8.2 Multiple User Isolation
- **Test Case**: Verify data isolation between users
- **Steps**:
  1. Register two users
  2. Login as first user and create reports
  3. Login as second user and get reports
- **Expected Result**: Second user cannot see first user's reports

## 9. Performance Tests

### 9.1 Concurrent Requests
- **Test Case**: Handle multiple concurrent requests
- **Steps**:
  1. Send multiple concurrent requests to various endpoints
  2. Verify all requests are processed correctly
- **Expected Result**: All requests are processed without errors

### 9.2 Large Data Handling
- **Test Case**: Handle large amounts of data
- **Steps**:
  1. Create multiple users and reports
  2. Verify database performance
- **Expected Result**: Database operations remain performant

## 10. Security Tests

### 10.1 JWT Expiration
- **Test Case**: Verify JWT token expiration
- **Steps**:
  1. Login to get token
  2. Wait for token to expire (24 hours)
  3. Attempt to access protected route
- **Expected Result**: 403 Forbidden response

### 10.2 Password Security
- **Test Case**: Verify password hashing
- **Steps**:
  1. Register a new user
  2. Check MongoDB for user record
  3. Verify password is hashed
- **Expected Result**: Password is stored as a hash, not plaintext

### 10.3 HIPAA Compliance
- **Test Case**: Verify HIPAA compliance flags
- **Steps**:
  1. Submit symptoms for diagnosis
  2. Verify HIPAA mode flag is set in request
- **Expected Result**: HIPAA mode flag is set to true

## Test Execution

### Prerequisites
1. MongoDB installed and running
2. Express server installed and configured
3. Test environment variables set

### Test Commands
```bash
# Start MongoDB
sudo systemctl start mongod

# Start Express server
cd 2ndopinionmd-express-server
npm run dev

# Test user registration
curl -X POST http://localhost:3000/api/auth/register -H "Content-Type: application/json" -d '{"email":"test@example.com","password":"password123","firstName":"Test","lastName":"User"}'

# Test user login
curl -X POST http://localhost:3000/api/auth/login -H "Content-Type: application/json" -d '{"email":"test@example.com","password":"password123"}'

# Test diagnosis (replace TOKEN with actual token)
curl -X POST http://localhost:3000/api/diagnose -H "Content-Type: application/json" -H "Authorization: Bearer TOKEN" -d '{"age":"35","sex":"female","symptoms":["fatigue","joint_pain","rash"],"durationMonths":"6"}'

# Test PDF generation (replace TOKEN and REPORT_ID with actual values)
curl -X POST http://localhost:3000/api/generate-pdf -H "Content-Type: application/json" -H "Authorization: Bearer TOKEN" -d '{"reportId":"REPORT_ID"}'

# Test get reports (replace TOKEN with actual token)
curl -X GET http://localhost:3000/api/reports -H "Authorization: Bearer TOKEN"
```

### Test Reporting
Document test results including:
1. Test case ID
2. Pass/Fail status
3. Actual results
4. Any issues encountered
5. Screenshots or logs as evidence

## Conclusion
This test plan provides comprehensive coverage of the 2ndOpinionMD Express server with MongoDB integration. By executing these tests, we can ensure that the server functions correctly, data is properly stored in and retrieved from MongoDB, and all requirements are met.
